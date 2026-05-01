use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    BlankPinyinInput,
    BlankHanziInput,
    CandidateLimitMustBePositive,
    ConversionUnavailable(String),
}

impl Display for EngineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlankPinyinInput => {
                write!(formatter, "pinyin input must contain at least one syllable")
            }
            Self::BlankHanziInput => {
                write!(formatter, "hanzi input must contain at least one character")
            }
            Self::CandidateLimitMustBePositive => {
                write!(formatter, "candidate limit must be greater than zero")
            }
            Self::ConversionUnavailable(message) => {
                write!(formatter, "conversion unavailable: {message}")
            }
        }
    }
}

impl Error for EngineError {}
