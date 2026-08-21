# ─────────────────────────────────────────────────────────────────────────────
# Écrit un secret dans le presse-papiers en le marquant « privé ».
#
# Pourquoi ce script plutôt que l'API d'Electron : vider le presse-papiers ne
# retire rien de l'historique Windows (Win+V). Pour qu'une valeur n'y entre
# jamais, il faut déposer, DANS LA MÊME SESSION de presse-papiers, le texte et
# les formats qui demandent aux moniteurs de l'ignorer. `clipboard.writeBuffer`
# d'Electron remplace tout le contenu à chaque appel : les deux ne peuvent pas
# cohabiter par ce biais.
#
# Le secret arrive par l'entrée standard, jamais en argument de ligne de
# commande : les arguments sont visibles par les autres processus.
# ─────────────────────────────────────────────────────────────────────────────

$ErrorActionPreference = 'Stop'

Add-Type -Namespace SshManager -Name NativeClipboard -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true)]
public static extern bool OpenClipboard(IntPtr hWndNewOwner);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool EmptyClipboard();

[DllImport("user32.dll", SetLastError = true)]
public static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

[DllImport("user32.dll", SetLastError = true)]
public static extern bool CloseClipboard();

[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern uint RegisterClipboardFormat(string lpszFormat);

[DllImport("kernel32.dll", SetLastError = true)]
public static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

[DllImport("kernel32.dll", SetLastError = true)]
public static extern IntPtr GlobalLock(IntPtr hMem);

[DllImport("kernel32.dll", SetLastError = true)]
public static extern bool GlobalUnlock(IntPtr hMem);
'@

$GMEM_MOVEABLE  = 0x0002
$CF_UNICODETEXT = 13

# Bloc mémoire global, transféré au système par SetClipboardData : il ne doit
# surtout pas être libéré ici en cas de succès.
function New-GlobalBlock {
    param([byte[]] $Bytes)

    $handle = [SshManager.NativeClipboard]::GlobalAlloc($GMEM_MOVEABLE, [UIntPtr]::new([uint64]$Bytes.Length))
    if ($handle -eq [IntPtr]::Zero) { throw "GlobalAlloc a échoué" }

    $pointer = [SshManager.NativeClipboard]::GlobalLock($handle)
    if ($pointer -eq [IntPtr]::Zero) { throw "GlobalLock a échoué" }

    [System.Runtime.InteropServices.Marshal]::Copy($Bytes, 0, $pointer, $Bytes.Length)
    [void][SshManager.NativeClipboard]::GlobalUnlock($handle)
    return $handle
}

$secret = [Console]::In.ReadToEnd()
if ($null -eq $secret) { $secret = '' }

# Texte UTF-16 terminé par un caractère nul, comme l'exige CF_UNICODETEXT
$textBytes = [System.Text.Encoding]::Unicode.GetBytes($secret + "`0")

if (-not [SshManager.NativeClipboard]::OpenClipboard([IntPtr]::Zero)) {
    throw "OpenClipboard a échoué (presse-papiers verrouillé par une autre application)"
}

try {
    [void][SshManager.NativeClipboard]::EmptyClipboard()

    if ([SshManager.NativeClipboard]::SetClipboardData($CF_UNICODETEXT, (New-GlobalBlock $textBytes)) -eq [IntPtr]::Zero) {
        throw "SetClipboardData a échoué pour le texte"
    }

    # Marqueurs reconnus par l'historique et le presse-papiers cloud.
    # Leur seule présence suffit ; la valeur transportée est ignorée.
    $markers = @(
        'ExcludeClipboardContentFromMonitorProcessing',
        'CanIncludeInClipboardHistory',
        'CanUploadToCloudClipboard'
    )
    $zero = [byte[]] @(0, 0, 0, 0)

    foreach ($marker in $markers) {
        $format = [SshManager.NativeClipboard]::RegisterClipboardFormat($marker)
        if ($format -eq 0) { throw "RegisterClipboardFormat a échoué pour $marker" }
        if ([SshManager.NativeClipboard]::SetClipboardData($format, (New-GlobalBlock $zero)) -eq [IntPtr]::Zero) {
            throw "SetClipboardData a échoué pour $marker"
        }
    }
}
finally {
    [void][SshManager.NativeClipboard]::CloseClipboard()
}

Write-Output 'OK'
