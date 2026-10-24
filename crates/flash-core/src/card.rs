//! Card content validation.

pub const MAX_SIDE_LEN: usize = 10_000;

/// Validated front/back text for a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    pub front: String,
    pub back: String,
}

/// Trims both sides, keeps them in one Unicode form (`text::nfc`), and
/// rejects empty or oversized content.
pub fn validate_card_text(front: &str, back: &str) -> Result<CardText, CardTextError> {
    let front = crate::text::nfc(front.trim());
    let back = crate::text::nfc(back.trim());
    if front.is_empty() {
        return Err(CardTextError::FrontEmpty);
    }
    if back.is_empty() {
        return Err(CardTextError::BackEmpty);
    }
    if front.len() > MAX_SIDE_LEN || back.len() > MAX_SIDE_LEN {
        return Err(CardTextError::TooLong);
    }
    Ok(CardText { front, back })
}

/// Why a card's text was refused. `Display` is the English a log or an
/// API client reads; a page words it in its reader's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardTextError {
    FrontEmpty,
    BackEmpty,
    /// A side is over `MAX_SIDE_LEN` bytes.
    TooLong,
}

impl std::fmt::Display for CardTextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CardTextError::FrontEmpty => f.write_str("card front is empty"),
            CardTextError::BackEmpty => f.write_str("card back is empty"),
            CardTextError::TooLong => write!(f, "card side exceeds {MAX_SIDE_LEN} bytes"),
        }
    }
}

impl std::error::Error for CardTextError {}
