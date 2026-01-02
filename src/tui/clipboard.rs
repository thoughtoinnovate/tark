//! Clipboard handler for the TUI
//!
//! Provides clipboard access for paste operations, detecting content type
//! (image, file path, text) and handling appropriately.
//!
//! Requirements: 11.1

#![allow(dead_code)]

use std::path::Path;

use arboard::Clipboard;

use super::attachments::{
    Attachment, AttachmentContent, AttachmentError, AttachmentType, ImageFormat,
};

/// Content retrieved from the clipboard
#[derive(Debug, Clone)]
pub enum ClipboardContent {
    /// Image data from clipboard (encoded as PNG bytes)
    Image(ImageData),
    /// File path detected in clipboard text
    FilePath(String),
    /// Plain text content
    Text(String),
    /// Clipboard is empty or content not available
    Empty,
}

/// Image data from clipboard
#[derive(Debug, Clone)]
pub struct ImageData {
    /// PNG-encoded image bytes
    pub bytes: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
}

/// Handler for clipboard operations
///
/// Provides methods to access clipboard content and detect content type.
/// Uses the `arboard` crate for cross-platform clipboard access.
///
/// Requirements: 11.1
pub struct ClipboardHandler {
    clipboard: Clipboard,
}

impl ClipboardHandler {
    /// Create a new clipboard handler
    ///
    /// Returns an error if clipboard access is not available.
    pub fn new() -> Result<Self, AttachmentError> {
        let clipboard =
            Clipboard::new().map_err(|e| AttachmentError::ClipboardError(e.to_string()))?;
        Ok(Self { clipboard })
    }

    /// Get content from the clipboard, detecting the content type
    ///
    /// Checks for content in this order:
    /// 1. Image data (returns ClipboardContent::Image)
    /// 2. Text that looks like a file path (returns ClipboardContent::FilePath)
    /// 3. Plain text (returns ClipboardContent::Text)
    /// 4. Empty clipboard (returns ClipboardContent::Empty)
    ///
    /// Requirements: 11.1
    pub fn get_content(&mut self) -> Result<ClipboardContent, AttachmentError> {
        // Try to get image first (Requirements 11.2)
        if let Some(image_data) = self.get_image()? {
            return Ok(ClipboardContent::Image(image_data));
        }

        // Try to get text (could be file path or plain text)
        if let Some(text) = self.get_text()? {
            // Check if text looks like a file path (Requirements 11.3)
            if is_file_path(&text) {
                return Ok(ClipboardContent::FilePath(text));
            }
            // Plain text (Requirements 11.4)
            return Ok(ClipboardContent::Text(text));
        }

        Ok(ClipboardContent::Empty)
    }

    /// Get image data from clipboard if available
    ///
    /// Returns Ok(Some(ImageData)) if an image was found,
    /// Ok(None) if no image was in the clipboard,
    /// or Err if there was an error accessing the clipboard.
    ///
    /// Requirements: 11.2
    pub fn get_image(&mut self) -> Result<Option<ImageData>, AttachmentError> {
        // Try to get image data from clipboard
        let image_data = match self.clipboard.get_image() {
            Ok(data) => data,
            Err(arboard::Error::ContentNotAvailable) => return Ok(None),
            Err(e) => return Err(AttachmentError::ClipboardError(e.to_string())),
        };

        // Convert to PNG bytes
        let bytes = encode_image_to_png(&image_data)?;
        let width = image_data.width as u32;
        let height = image_data.height as u32;

        Ok(Some(ImageData {
            bytes,
            width,
            height,
        }))
    }

    /// Get text from clipboard if available
    ///
    /// Returns Ok(Some(text)) if text was found,
    /// Ok(None) if no text was in the clipboard,
    /// or Err if there was an error accessing the clipboard.
    pub fn get_text(&mut self) -> Result<Option<String>, AttachmentError> {
        match self.clipboard.get_text() {
            Ok(text) => {
                if text.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(text))
                }
            }
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(AttachmentError::ClipboardError(e.to_string())),
        }
    }

    /// Create an attachment from clipboard image data
    ///
    /// Generates a filename with timestamp and creates an Attachment
    /// with the image data encoded as base64.
    ///
    /// Requirements: 11.2, 11.6
    pub fn image_to_attachment(image_data: ImageData) -> Attachment {
        use super::attachments::base64_encode;

        // Generate filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("clipboard_{}.png", timestamp);

        // Encode as base64
        let encoded = base64_encode(&image_data.bytes);
        let size = image_data.bytes.len() as u64;

        let file_type = AttachmentType::Image {
            format: ImageFormat::Png,
            width: image_data.width,
            height: image_data.height,
        };

        Attachment::new(
            filename,
            file_type,
            size,
            AttachmentContent::Base64(encoded),
        )
    }
}

/// Check if a string looks like a file path
///
/// Detects:
/// - Absolute paths (starting with / on Unix or drive letter on Windows)
/// - Home directory paths (starting with ~)
/// - Relative paths that exist on disk
///
/// Requirements: 11.3
fn is_file_path(text: &str) -> bool {
    let text = text.trim();

    // Empty or multi-line text is not a file path
    if text.is_empty() || text.contains('\n') {
        return false;
    }

    // Check for absolute paths
    if text.starts_with('/') || text.starts_with('~') {
        // Verify it looks like a path (not just a slash)
        return text.len() > 1 && !text.contains('\t');
    }

    // Check for Windows-style paths (C:\, D:\, etc.)
    if text.len() >= 3 {
        let chars: Vec<char> = text.chars().collect();
        if chars[0].is_ascii_alphabetic()
            && chars[1] == ':'
            && (chars[2] == '\\' || chars[2] == '/')
        {
            return true;
        }
    }

    // Check if it's a relative path that exists
    let path = Path::new(text);
    if path.exists() {
        return true;
    }

    // Check for common file path patterns (contains path separators and extension)
    if (text.contains('/') || text.contains('\\')) && text.contains('.') {
        // Looks like a path with extension
        return true;
    }

    false
}

/// Encode clipboard image data to PNG bytes
fn encode_image_to_png(image_data: &arboard::ImageData) -> Result<Vec<u8>, AttachmentError> {
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;

    // Create an image buffer from the clipboard data
    let width = image_data.width as u32;
    let height = image_data.height as u32;

    // arboard returns RGBA data
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, image_data.bytes.to_vec()).ok_or_else(|| {
            AttachmentError::ImageError("Failed to create image buffer".to_string())
        })?;

    // Encode to PNG
    let mut bytes = Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| AttachmentError::ImageError(e.to_string()))?;

    Ok(bytes.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_file_path_absolute_unix() {
        assert!(is_file_path("/home/user/file.txt"));
        assert!(is_file_path("/etc/config"));
        assert!(is_file_path("~/Documents/file.txt"));
    }

    #[test]
    fn test_is_file_path_absolute_windows() {
        assert!(is_file_path("C:\\Users\\file.txt"));
        assert!(is_file_path("D:/Documents/file.txt"));
    }

    #[test]
    fn test_is_file_path_relative_with_extension() {
        assert!(is_file_path("src/main.rs"));
        assert!(is_file_path("./config/settings.json"));
        assert!(is_file_path("../parent/file.txt"));
    }

    #[test]
    fn test_is_file_path_not_path() {
        assert!(!is_file_path("Hello, world!"));
        assert!(!is_file_path("This is a sentence."));
        assert!(!is_file_path(""));
        assert!(!is_file_path("   "));
        assert!(!is_file_path("line1\nline2"));
    }

    #[test]
    fn test_is_file_path_edge_cases() {
        // Single slash is not a valid file path for our purposes
        assert!(!is_file_path("/"));
        // Just a tilde is not a valid path
        assert!(!is_file_path("~"));
    }

    #[test]
    fn test_image_to_attachment() {
        // Create mock image data
        let image_data = ImageData {
            bytes: vec![0u8; 100], // Dummy bytes
            width: 10,
            height: 10,
        };

        let attachment = ClipboardHandler::image_to_attachment(image_data);

        assert!(attachment.filename.starts_with("clipboard_"));
        assert!(attachment.filename.ends_with(".png"));
        assert!(matches!(
            attachment.file_type,
            AttachmentType::Image {
                format: ImageFormat::Png,
                width: 10,
                height: 10
            }
        ));
        assert!(matches!(attachment.content, AttachmentContent::Base64(_)));
    }

    // Note: Clipboard tests that require actual clipboard access are difficult
    // to run in CI environments. The following test is marked as ignored.
    #[test]
    #[ignore]
    fn test_clipboard_handler_new() {
        // This test requires a display server and clipboard access
        let result = ClipboardHandler::new();
        assert!(result.is_ok());
    }

    #[test]
    #[ignore]
    fn test_clipboard_get_content() {
        // This test requires a display server and clipboard access
        let mut handler = ClipboardHandler::new().unwrap();
        let result = handler.get_content();
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating valid image dimensions
    fn image_dimensions() -> impl Strategy<Value = (u32, u32)> {
        (1u32..100, 1u32..100)
    }

    // Strategy for generating valid RGBA image bytes
    fn rgba_bytes(width: u32, height: u32) -> Vec<u8> {
        let size = (width * height * 4) as usize;
        vec![128u8; size] // Gray pixels with full alpha
    }

    // Strategy for generating file path strings
    fn file_path_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Unix absolute paths
            "[a-z]{1,10}/[a-z]{1,10}\\.[a-z]{2,4}".prop_map(|s| format!("/{}", s)),
            // Home directory paths
            "[a-z]{1,10}/[a-z]{1,10}\\.[a-z]{2,4}".prop_map(|s| format!("~/{}", s)),
            // Windows paths
            "[A-Z]".prop_map(|c| format!("{}:\\Users\\file.txt", c)),
            // Relative paths with extension
            "[a-z]{1,10}/[a-z]{1,10}\\.[a-z]{2,4}",
        ]
    }

    // Strategy for generating plain text (not file paths)
    fn plain_text_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple sentences
            "[A-Za-z ]{10,50}",
            // Text with punctuation
            "[A-Za-z ,\\.!?]{10,50}",
            // Multi-line text
            "[A-Za-z ]{5,20}\n[A-Za-z ]{5,20}",
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// **Feature: tui-llm-integration, Property 11: Clipboard Paste Handling**
        /// **Validates: Requirements 11.2, 11.3, 11.4, 11.5, 11.7**
        ///
        /// *For any* clipboard paste action (Ctrl+V/Cmd+V), the TUI SHALL detect content type
        /// (image, file path, text) and handle appropriately: images and files become attachments
        /// shown in the attachment bar, text is pasted into input, and all pasted attachments
        /// are included when the message is sent.

        /// Property 11a: Image data creates valid attachment
        /// *For any* valid image dimensions, creating an ImageData and converting to attachment
        /// SHALL produce an attachment with PNG format, correct dimensions, and base64 content.
        #[test]
        fn prop_image_to_attachment_creates_valid_attachment((width, height) in image_dimensions()) {
            let bytes = rgba_bytes(width, height);
            let image_data = ImageData {
                bytes: bytes.clone(),
                width,
                height,
            };

            let attachment = ClipboardHandler::image_to_attachment(image_data);

            // Verify filename format
            prop_assert!(attachment.filename.starts_with("clipboard_"));
            prop_assert!(attachment.filename.ends_with(".png"));

            // Verify file type is PNG image with correct dimensions
            let is_correct_image = matches!(
                &attachment.file_type,
                AttachmentType::Image {
                    format: ImageFormat::Png,
                    width: w,
                    height: h
                } if *w == width && *h == height
            );
            prop_assert!(is_correct_image, "Expected PNG image with dimensions {}x{}", width, height);

            // Verify content is base64 encoded
            prop_assert!(matches!(attachment.content, AttachmentContent::Base64(_)));

            // Verify size matches original bytes
            prop_assert_eq!(attachment.size, bytes.len() as u64);
        }

        /// Property 11b: File paths are correctly detected
        /// *For any* string that looks like a file path (absolute, home-relative, or with
        /// path separators and extension), is_file_path SHALL return true.
        #[test]
        fn prop_file_paths_detected(path in file_path_strategy()) {
            prop_assert!(is_file_path(&path), "Expected '{}' to be detected as file path", path);
        }

        /// Property 11c: Plain text is not detected as file path
        /// *For any* plain text string (without path separators or file-like patterns),
        /// is_file_path SHALL return false.
        #[test]
        fn prop_plain_text_not_file_path(text in plain_text_strategy()) {
            // Filter out text that accidentally looks like a path
            prop_assume!(!text.contains('/') || text.contains('\n'));
            prop_assume!(!text.contains('\\'));
            prop_assume!(!text.starts_with('~'));

            prop_assert!(!is_file_path(&text), "Expected '{}' to NOT be detected as file path", text);
        }

        /// Property 11d: Empty and whitespace-only strings are not file paths
        /// *For any* string composed entirely of whitespace, is_file_path SHALL return false.
        #[test]
        fn prop_whitespace_not_file_path(spaces in 0usize..20) {
            let whitespace = " ".repeat(spaces);
            prop_assert!(!is_file_path(&whitespace));
        }

        /// Property 11e: Attachment size is preserved
        /// *For any* image data, the attachment size SHALL equal the original byte count.
        #[test]
        fn prop_attachment_size_preserved((width, height) in image_dimensions()) {
            let bytes = rgba_bytes(width, height);
            let original_size = bytes.len() as u64;

            let image_data = ImageData {
                bytes,
                width,
                height,
            };

            let attachment = ClipboardHandler::image_to_attachment(image_data);
            prop_assert_eq!(attachment.size, original_size);
        }
    }
}
