use std::{
    ffi::{OsStr, OsString},
    os::unix::ffi::{OsStrExt, OsStringExt},
};

/// Retains raw filename bytes; rendering never feeds a lossy name back to disk.
pub struct FilenameEditor {
    bytes: Vec<u8>,
}

impl FilenameEditor {
    pub fn new(name: &OsStr) -> Self {
        Self {
            bytes: name.as_bytes().to_vec(),
        }
    }

    pub fn name(&self) -> OsString {
        OsString::from_vec(self.bytes.clone())
    }

    pub fn append(&mut self, character: char) {
        self.bytes
            .extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
    }

    pub fn backspace(&mut self) {
        // Delete the final Unicode scalar when valid, or one raw byte otherwise.
        let start = self.bytes.len().saturating_sub(4);
        let length = (start..self.bytes.len())
            .find_map(|start| {
                std::str::from_utf8(&self.bytes[start..])
                    .ok()
                    .filter(|suffix| suffix.chars().count() == 1)
                    .map(|_| self.bytes.len() - start)
            })
            .unwrap_or(1);
        self.bytes.truncate(self.bytes.len().saturating_sub(length));
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_unicode_and_raw_bytes_without_lossy_conversion() {
        let original = OsStr::from_bytes(b"raw-\xff");
        let mut editor = FilenameEditor::new(original);
        assert_eq!(editor.name(), original);
        editor.append('é');
        editor.backspace();
        assert_eq!(editor.name(), original);
        editor.backspace();
        assert_eq!(editor.name(), "raw-");
        editor.clear();
        editor.backspace();
        assert!(editor.name().is_empty());
    }
}
