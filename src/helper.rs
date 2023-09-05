// various functions useful for internal parsing or other things

/// Error for any kind of parsing
pub enum ParseError {
    /// Generic error -> something is wrong
    UnexpectedValue
}

pub fn hex_to_rgb(hex: String) -> Result<image::Rgb<u8>, ParseError> {
    
}
