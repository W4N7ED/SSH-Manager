//! Copie d'un flux vers un autre avec avancement et annulation : sert aux
//! téléchargements et aux envois, en SFTP comme en FTP.

use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const CHUNK_SIZE: usize = 64 * 1024;

/// Ce qu'un transfert reçoit de l'appelant : où signaler l'avancement, et le
/// drapeau d'annulation.
pub struct TransferHooks<'a> {
    pub progress: &'a mut (dyn FnMut(u64) + Send),
    pub cancelled: &'a AtomicBool,
}

/// Supprime un fichier local laissé à moitié écrit par un transfert annulé.
pub async fn discard_partial_local(path: &std::path::Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        tracing::warn!(path = %path.display(), %error, "partial download could not be removed");
    }
}

/// Fin d'une copie : complète, ou interrompue à la demande de l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOutcome {
    Completed(u64),
    Cancelled,
}

/// Copie `reader` dans `writer` par blocs. Après chaque bloc, `progress`
/// reçoit le total copié et `cancelled` est consulté.
pub async fn copy<R, W>(
    reader: &mut R,
    writer: &mut W,
    progress: &mut (dyn FnMut(u64) + Send),
    cancelled: &AtomicBool,
) -> std::io::Result<CopyOutcome>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut copied = 0u64;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(CopyOutcome::Cancelled);
        }
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            writer.flush().await?;
            return Ok(CopyOutcome::Completed(copied));
        }
        writer.write_all(&buffer[..read]).await?;
        copied += read as u64;
        progress(copied);
    }
}

/// Pourcentage affichable, si la taille totale est connue et non nulle.
pub fn percent(done: u64, total: Option<u64>) -> Option<f32> {
    total
        .filter(|total| *total > 0)
        .map(|total| (done as f32 / total as f32).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn should_copy_everything_and_report_progress() {
        let data: Vec<u8> = (0..200_000u32).map(|value| value as u8).collect();
        let mut reader = data.as_slice();
        let mut writer = Vec::new();
        let mut reports = Vec::new();
        let outcome = run(copy(
            &mut reader,
            &mut writer,
            &mut |done| reports.push(done),
            &AtomicBool::new(false),
        ))
        .unwrap();
        assert_eq!(outcome, CopyOutcome::Completed(200_000));
        assert_eq!(writer, data);
        assert_eq!(reports.last(), Some(&200_000));
        assert!(
            reports.windows(2).all(|pair| pair[0] < pair[1]),
            "avancement croissant"
        );
    }

    #[test]
    fn should_stop_as_soon_as_cancelled() {
        let data = vec![7u8; 500_000];
        let mut reader = data.as_slice();
        let mut writer = Vec::new();
        let cancelled = AtomicBool::new(false);
        let outcome = run(copy(
            &mut reader,
            &mut writer,
            // Annulé dès le premier bloc copié.
            &mut |_| cancelled.store(true, Ordering::Relaxed),
            &cancelled,
        ))
        .unwrap();
        assert_eq!(outcome, CopyOutcome::Cancelled);
        assert_eq!(writer.len(), CHUNK_SIZE);
    }

    #[test]
    fn should_compute_a_bounded_percentage() {
        assert_eq!(percent(50, Some(200)), Some(0.25));
        assert_eq!(percent(300, Some(200)), Some(1.0));
        assert_eq!(percent(10, None), None);
        assert_eq!(percent(0, Some(0)), None);
    }
}
