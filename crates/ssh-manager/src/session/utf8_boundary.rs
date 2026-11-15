//! Recolle les caractères UTF-8 coupés entre deux paquets réseau.
//!
//! vte 0.15 sait reprendre un caractère incomplet, mais en reprenant il avale
//! le caractère ASCII qui le suit (« été » coupé après le premier octet donne
//! « éé »). On ne lui transmet donc que des paquets dont la fin est complète :
//! les octets d'un caractère inachevé attendent le paquet suivant.

#[derive(Debug, Default)]
pub struct Utf8Boundary {
    carry: Vec<u8>,
}

impl Utf8Boundary {
    /// Octets prêts à être analysés : la fin retenue du paquet précédent, puis
    /// ce paquet, sans son éventuel caractère inachevé.
    pub fn complete(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut joined = std::mem::take(&mut self.carry);
        joined.extend_from_slice(bytes);
        let cut = incomplete_tail_start(&joined);
        self.carry = joined.split_off(cut);
        joined
    }
}

/// Position du premier octet d'un caractère inachevé en fin de tampon, ou la
/// longueur du tampon si la fin est complète.
fn incomplete_tail_start(bytes: &[u8]) -> usize {
    let length = bytes.len();
    for back in 1..=length.min(3) {
        let byte = bytes[length - back];
        let is_continuation = byte & 0b1100_0000 == 0b1000_0000;
        if is_continuation {
            continue;
        }
        let expected = match byte {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        };
        return if expected > back {
            length - back
        } else {
            length
        };
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_pass_complete_text_through_unchanged() {
        assert_eq!(
            Utf8Boundary::default().complete("été".as_bytes()),
            "été".as_bytes()
        );
    }

    #[test]
    fn should_hold_back_a_split_character_until_the_next_packet() {
        let mut boundary = Utf8Boundary::default();
        let bytes = "été".as_bytes();
        assert_eq!(boundary.complete(&bytes[..1]), b"");
        assert_eq!(boundary.complete(&bytes[1..4]), "ét".as_bytes());
        assert_eq!(boundary.complete(&bytes[4..]), "é".as_bytes());
    }

    #[test]
    fn should_hold_back_a_four_byte_character_split_after_three_bytes() {
        let mut boundary = Utf8Boundary::default();
        let emoji = "🙂".as_bytes();
        assert_eq!(boundary.complete(&emoji[..3]), b"");
        assert_eq!(boundary.complete(&emoji[3..]), emoji);
    }

    #[test]
    fn should_not_hold_invalid_bytes_forever() {
        // Une suite de continuations orphelines n'attend rien : elle passe telle quelle.
        let mut boundary = Utf8Boundary::default();
        assert_eq!(
            boundary.complete(&[0x80, 0x80, 0x80, 0x80]),
            [0x80, 0x80, 0x80, 0x80]
        );
    }
}
