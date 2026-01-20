//! Attachment manager for the TUI
//!
//! Handles file attachments and clipboard images for chat messages.
//! Supports various file types including images, text, code, and documents.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

/// Configuration for attachment handling
#[derive(Debug, Clone)]
pub struct AttachmentConfig {
    /// Maximum file size in bytes (default: 10MB)
    pub max_attachment_size: u64,
    /// Maximum number of attachments per message (default: 10)
    pub max_attachments: usize,
    /// Temporary directory for storing processed attachments
    pub temp_dir: PathBuf,
}

impl Default for AttachmentConfig {
    fn default() -> Self {
        Self {
            max_attachment_size: 10 * 1024 * 1024, // 10MB
            max_attachments: 10,
            temp_dir: std::env::temp_dir().join("tark-attachments"),
        }
    }
}

/// Supported image formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl ImageFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::WebP => "image/webp",
        }
    }

    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Gif => "gif",
            ImageFormat::WebP => "webp",
        }
    }

    /// Detect format from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "gif" => Some(ImageFormat::Gif),
            "webp" => Some(ImageFormat::WebP),
            _ => None,
        }
    }
}

/// Document formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    Pdf,
    Markdown,
    PlainText,
    RestructuredText,
}

impl DocumentFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            DocumentFormat::Pdf => "application/pdf",
            DocumentFormat::Markdown => "text/markdown",
            DocumentFormat::PlainText => "text/plain",
            DocumentFormat::RestructuredText => "text/x-rst",
        }
    }
}

/// Data formats (structured data)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Yaml,
    Toml,
    Xml,
}

impl DataFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            DataFormat::Json => "application/json",
            DataFormat::Yaml => "application/x-yaml",
            DataFormat::Toml => "application/toml",
            DataFormat::Xml => "application/xml",
        }
    }
}

/// Type of attachment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentType {
    /// Image attachment with format and dimensions
    Image {
        format: ImageFormat,
        width: u32,
        height: u32,
    },
    /// Text file with optional language for syntax highlighting
    Text { language: Option<String> },
    /// Document (PDF, markdown, etc.)
    Document { format: DocumentFormat },
    /// Structured data (JSON, YAML, etc.)
    Data { format: DataFormat },
}

impl AttachmentType {
    /// Get a display icon for this attachment type
    pub fn icon(&self) -> &'static str {
        match self {
            AttachmentType::Image { .. } => "📷",
            AttachmentType::Text { .. } => "📄",
            AttachmentType::Document { format } => match format {
                DocumentFormat::Pdf => "📕",
                _ => "📝",
            },
            AttachmentType::Data { .. } => "📊",
        }
    }

    /// Get the MIME type for this attachment
    pub fn mime_type(&self) -> &'static str {
        match self {
            AttachmentType::Image { format, .. } => format.mime_type(),
            AttachmentType::Text { .. } => "text/plain",
            AttachmentType::Document { format } => format.mime_type(),
            AttachmentType::Data { format } => format.mime_type(),
        }
    }
}

/// Content of an attachment
#[derive(Debug, Clone)]
pub enum AttachmentContent {
    /// Base64 encoded content (for images)
    Base64(String),
    /// Raw text content
    Text(String),
    /// File path (for large files, read on send)
    Path(PathBuf),
}

impl AttachmentContent {
    /// Get the size of the content in bytes
    pub fn size(&self) -> u64 {
        match self {
            AttachmentContent::Base64(s) => s.len() as u64,
            AttachmentContent::Text(s) => s.len() as u64,
            AttachmentContent::Path(p) => fs::metadata(p).map(|m| m.len()).unwrap_or(0),
        }
    }
}

/// A file attachment
#[derive(Debug, Clone)]
pub struct Attachment {
    /// Unique identifier
    pub id: Uuid,
    /// Original filename
    pub filename: String,
    /// Type of attachment
    pub file_type: AttachmentType,
    /// Size in bytes
    pub size: u64,
    /// Content
    pub content: AttachmentContent,
}

impl Attachment {
    /// Create a new attachment
    pub fn new(
        filename: String,
        file_type: AttachmentType,
        size: u64,
        content: AttachmentContent,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename,
            file_type,
            size,
            content,
        }
    }

    /// Get a preview string for display
    pub fn preview(&self) -> String {
        let size_str = format_size(self.size);
        format!("{} {} ({})", self.file_type.icon(), self.filename, size_str)
    }

    /// Convert to a message attachment for sending
    pub fn to_message_attachment(&self) -> MessageAttachment {
        MessageAttachment {
            filename: self.filename.clone(),
            mime_type: self.file_type.mime_type().to_string(),
            content: self.content.clone(),
        }
    }
}

/// Attachment ready to be sent with a message
#[derive(Debug, Clone)]
pub struct MessageAttachment {
    /// Filename
    pub filename: String,
    /// MIME type
    pub mime_type: String,
    /// Content
    pub content: AttachmentContent,
}

impl MessageAttachment {
    /// Check if this is an image attachment
    pub fn is_image(&self) -> bool {
        self.mime_type.starts_with("image/")
    }

    /// Get the content as text (for non-image attachments)
    pub fn as_text(&self) -> Option<&str> {
        match &self.content {
            AttachmentContent::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Get the content as base64 (for image attachments)
    pub fn as_base64(&self) -> Option<&str> {
        match &self.content {
            AttachmentContent::Base64(s) => Some(s),
            _ => None,
        }
    }
}

/// Format a size in bytes to a human-readable string
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Error types for attachment operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// File not found
    FileNotFound(String),
    /// File too large
    FileTooLarge { size: u64, max: u64 },
    /// Too many attachments
    TooManyAttachments { count: usize, max: usize },
    /// Unsupported file type
    UnsupportedFileType(String),
    /// IO error
    IoError(String),
    /// Clipboard error
    ClipboardError(String),
    /// Image processing error
    ImageError(String),
}

impl std::fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentError::FileNotFound(path) => write!(f, "File not found: {}", path),
            AttachmentError::FileTooLarge { size, max } => {
                write!(
                    f,
                    "File too large: {} (max: {})",
                    format_size(*size),
                    format_size(*max)
                )
            }
            AttachmentError::TooManyAttachments { count, max } => {
                write!(f, "Too many attachments: {} (max: {})", count, max)
            }
            AttachmentError::UnsupportedFileType(ext) => {
                write!(f, "Unsupported file type: {}", ext)
            }
            AttachmentError::IoError(msg) => write!(f, "IO error: {}", msg),
            AttachmentError::ClipboardError(msg) => write!(f, "Clipboard error: {}", msg),
            AttachmentError::ImageError(msg) => write!(f, "Image error: {}", msg),
        }
    }
}

impl std::error::Error for AttachmentError {}

/// Attachment manager for handling pending attachments
#[derive(Debug)]
pub struct AttachmentManager {
    /// Pending attachments for the next message
    pending: Vec<Attachment>,
    /// Configuration
    config: AttachmentConfig,
}

impl Default for AttachmentManager {
    fn default() -> Self {
        Self::new(AttachmentConfig::default())
    }
}

impl AttachmentManager {
    /// Create a new attachment manager with the given configuration
    pub fn new(config: AttachmentConfig) -> Self {
        Self {
            pending: Vec::new(),
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &AttachmentConfig {
        &self.config
    }

    /// Get the number of pending attachments
    pub fn count(&self) -> usize {
        self.pending.len()
    }

    /// Check if there are any pending attachments
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Get all pending attachments
    pub fn pending(&self) -> &[Attachment] {
        &self.pending
    }

    /// Get a pending attachment by ID
    pub fn get(&self, id: Uuid) -> Option<&Attachment> {
        self.pending.iter().find(|a| a.id == id)
    }

    /// Add an attachment
    pub fn add(&mut self, attachment: Attachment) -> Result<(), AttachmentError> {
        // Check count limit
        if self.pending.len() >= self.config.max_attachments {
            return Err(AttachmentError::TooManyAttachments {
                count: self.pending.len() + 1,
                max: self.config.max_attachments,
            });
        }

        // Check size limit
        if attachment.size > self.config.max_attachment_size {
            return Err(AttachmentError::FileTooLarge {
                size: attachment.size,
                max: self.config.max_attachment_size,
            });
        }

        self.pending.push(attachment);
        Ok(())
    }

    /// Remove an attachment by ID
    pub fn remove(&mut self, id: Uuid) -> Option<Attachment> {
        if let Some(pos) = self.pending.iter().position(|a| a.id == id) {
            Some(self.pending.remove(pos))
        } else {
            None
        }
    }

    /// Remove an attachment by index
    pub fn remove_at(&mut self, index: usize) -> Option<Attachment> {
        if index < self.pending.len() {
            Some(self.pending.remove(index))
        } else {
            None
        }
    }

    /// Clear all pending attachments
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Take all pending attachments (clears the pending list)
    pub fn take_all(&mut self) -> Vec<Attachment> {
        std::mem::take(&mut self.pending)
    }

    /// Get the total size of all pending attachments
    pub fn total_size(&self) -> u64 {
        self.pending.iter().map(|a| a.size).sum()
    }

    /// Check if adding a file of the given size would exceed limits
    pub fn can_add(&self, size: u64) -> Result<(), AttachmentError> {
        if self.pending.len() >= self.config.max_attachments {
            return Err(AttachmentError::TooManyAttachments {
                count: self.pending.len() + 1,
                max: self.config.max_attachments,
            });
        }

        if size > self.config.max_attachment_size {
            return Err(AttachmentError::FileTooLarge {
                size,
                max: self.config.max_attachment_size,
            });
        }

        Ok(())
    }

    /// Attach a file from the given path
    pub fn attach_file(&mut self, path: &Path) -> Result<Attachment, AttachmentError> {
        // Check if file exists
        if !path.exists() {
            return Err(AttachmentError::FileNotFound(path.display().to_string()));
        }

        // Get file metadata
        let metadata = fs::metadata(path).map_err(|e| AttachmentError::IoError(e.to_string()))?;

        let size = metadata.len();

        // Check limits before processing
        self.can_add(size)?;

        // Detect file type
        let file_type = detect_file_type(path)?;

        // Read content based on type
        let content = read_file_content(path, &file_type)?;

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let attachment = Attachment::new(filename, file_type, size, content);

        self.add(attachment.clone())?;

        Ok(attachment)
    }

    /// Attach an image from the clipboard
    ///
    /// Returns Ok(Some(attachment)) if an image was found and attached,
    /// Ok(None) if no image was in the clipboard,
    /// or Err if there was an error processing the image.
    pub fn attach_clipboard(&mut self) -> Result<Option<Attachment>, AttachmentError> {
        // Try to get image from clipboard
        let image_data = get_clipboard_image()?;

        let Some((bytes, format)) = image_data else {
            return Ok(None);
        };

        // Check size limit
        let size = bytes.len() as u64;
        self.can_add(size)?;

        // Get image dimensions
        let (width, height) = get_image_dimensions_from_bytes(&bytes)?;

        // Generate filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("clipboard_{}.{}", timestamp, format.extension());

        // Encode as base64
        let encoded = base64_encode(&bytes);

        let file_type = AttachmentType::Image {
            format,
            width,
            height,
        };

        let attachment = Attachment::new(
            filename,
            file_type,
            size,
            AttachmentContent::Base64(encoded),
        );

        self.add(attachment.clone())?;

        Ok(Some(attachment))
    }

    /// Save clipboard image to a temporary file
    ///
    /// This is useful when you need the image as a file rather than in memory.
    pub fn save_clipboard_to_temp(&self) -> Result<Option<PathBuf>, AttachmentError> {
        let image_data = get_clipboard_image()?;

        let Some((bytes, format)) = image_data else {
            return Ok(None);
        };

        // Ensure temp directory exists
        fs::create_dir_all(&self.config.temp_dir)
            .map_err(|e| AttachmentError::IoError(e.to_string()))?;

        // Generate filename with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("clipboard_{}.{}", timestamp, format.extension());
        let path = self.config.temp_dir.join(filename);

        // Write to file
        fs::write(&path, &bytes).map_err(|e| AttachmentError::IoError(e.to_string()))?;

        Ok(Some(path))
    }

    /// Prepare all pending attachments for sending with a message
    ///
    /// This converts all pending attachments to MessageAttachment format
    /// and clears the pending list.
    pub fn prepare_for_send(&mut self) -> Vec<MessageAttachment> {
        let attachments = self.take_all();
        attachments
            .iter()
            .map(|a| a.to_message_attachment())
            .collect()
    }

    /// Check if any pending attachments are images
    pub fn has_images(&self) -> bool {
        self.pending
            .iter()
            .any(|a| matches!(a.file_type, AttachmentType::Image { .. }))
    }
}

/// Detect the file type from a path
pub fn detect_file_type(path: &Path) -> Result<AttachmentType, AttachmentError> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Check for image formats
    if let Some(format) = ImageFormat::from_extension(&extension) {
        // Try to get image dimensions
        let (width, height) = get_image_dimensions(path).unwrap_or((0, 0));
        return Ok(AttachmentType::Image {
            format,
            width,
            height,
        });
    }

    // Check for document formats
    match extension.as_str() {
        "pdf" => {
            return Ok(AttachmentType::Document {
                format: DocumentFormat::Pdf,
            })
        }
        "md" | "markdown" => {
            return Ok(AttachmentType::Document {
                format: DocumentFormat::Markdown,
            })
        }
        "txt" => {
            return Ok(AttachmentType::Document {
                format: DocumentFormat::PlainText,
            })
        }
        "rst" => {
            return Ok(AttachmentType::Document {
                format: DocumentFormat::RestructuredText,
            })
        }
        _ => {}
    }

    // Check for data formats
    match extension.as_str() {
        "json" => {
            return Ok(AttachmentType::Data {
                format: DataFormat::Json,
            })
        }
        "yaml" | "yml" => {
            return Ok(AttachmentType::Data {
                format: DataFormat::Yaml,
            })
        }
        "toml" => {
            return Ok(AttachmentType::Data {
                format: DataFormat::Toml,
            })
        }
        "xml" => {
            return Ok(AttachmentType::Data {
                format: DataFormat::Xml,
            })
        }
        _ => {}
    }

    // Check for code files (text with language detection)
    let language = detect_language(&extension);
    Ok(AttachmentType::Text { language })
}

/// Detect programming language from file extension
pub fn detect_language(extension: &str) -> Option<String> {
    let lang = match extension {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "ts" => "typescript",
        "jsx" => "javascript",
        "tsx" => "typescript",
        "go" => "go",
        "java" => "java",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" | "hpp" => "cpp",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "lua" => "lua",
        "sh" | "bash" => "bash",
        "zsh" => "zsh",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "vue" => "vue",
        "svelte" => "svelte",
        "elm" => "elm",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "clj" | "cljs" => "clojure",
        "r" => "r",
        "jl" => "julia",
        "nim" => "nim",
        "zig" => "zig",
        "v" => "v",
        "d" => "d",
        "dart" => "dart",
        "groovy" => "groovy",
        "pl" | "pm" => "perl",
        "tcl" => "tcl",
        "vim" => "vim",
        "dockerfile" => "dockerfile",
        "makefile" => "makefile",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Resolve a file path (absolute or relative to cwd)
///
/// This function handles:
/// - Absolute paths (starting with /)
/// - Home directory expansion (starting with ~)
/// - Relative paths (resolved from current working directory)
pub fn resolve_file_path(path_str: &str) -> Result<PathBuf, AttachmentError> {
    let path_str = path_str.trim();

    if path_str.is_empty() {
        return Err(AttachmentError::FileNotFound("Empty path".to_string()));
    }

    let path = if let Some(stripped) = path_str.strip_prefix('~') {
        // Expand home directory
        let home = dirs::home_dir().ok_or_else(|| {
            AttachmentError::IoError("Could not determine home directory".to_string())
        })?;
        home.join(stripped.trim_start_matches('/'))
    } else if path_str.starts_with('/') {
        // Absolute path
        PathBuf::from(path_str)
    } else {
        // Relative path - resolve from current directory
        let cwd = std::env::current_dir().map_err(|e| AttachmentError::IoError(e.to_string()))?;
        cwd.join(path_str)
    };

    // Canonicalize to resolve .. and . components
    let canonical = path
        .canonicalize()
        .map_err(|_| AttachmentError::FileNotFound(path_str.to_string()))?;

    Ok(canonical)
}

/// Parse @filepath references from input text
///
/// Returns a list of file paths found in the input.
/// The @filepath syntax supports:
/// - @path/to/file.txt
/// - @"path with spaces/file.txt"
/// - @'path with spaces/file.txt'
pub fn parse_file_references(input: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '@' && (i == 0 || chars[i - 1].is_whitespace()) {
            i += 1;
            if i >= chars.len() {
                break;
            }

            let path = if chars[i] == '"' || chars[i] == '\'' {
                // Quoted path
                let quote = chars[i];
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                if i > start {
                    let path: String = chars[start..i].iter().collect();
                    i += 1; // Skip closing quote
                    Some(path)
                } else {
                    None
                }
            } else {
                // Unquoted path - read until whitespace
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                if i > start {
                    let path: String = chars[start..i].iter().collect();
                    Some(path)
                } else {
                    None
                }
            };

            if let Some(p) = path {
                if !p.is_empty() {
                    paths.push(p);
                }
            }
        } else {
            i += 1;
        }
    }

    paths
}

/// Remove @filepath references from input text
///
/// Returns the input with all @filepath references removed.
pub fn remove_file_references(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '@' && (i == 0 || chars[i - 1].is_whitespace()) {
            let start = i;
            i += 1;
            if i >= chars.len() {
                result.push('@');
                break;
            }

            let skip_to = if chars[i] == '"' || chars[i] == '\'' {
                // Quoted path
                let quote = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1; // Skip closing quote
                }
                i
            } else {
                // Unquoted path - read until whitespace
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                i
            };

            // Check if we actually found a path
            if skip_to > start + 1 {
                // Skip the @filepath, but keep any trailing whitespace
                continue;
            } else {
                // Not a valid @filepath, keep the @
                result.push('@');
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    // Clean up extra whitespace
    let result = result.split_whitespace().collect::<Vec<_>>().join(" ");
    result
}

/// Get image dimensions from a file
fn get_image_dimensions(path: &Path) -> Result<(u32, u32), AttachmentError> {
    use image::GenericImageView;

    let img = image::open(path).map_err(|e| AttachmentError::ImageError(e.to_string()))?;

    Ok(img.dimensions())
}

/// Get image dimensions from bytes
fn get_image_dimensions_from_bytes(bytes: &[u8]) -> Result<(u32, u32), AttachmentError> {
    use image::GenericImageView;

    let img =
        image::load_from_memory(bytes).map_err(|e| AttachmentError::ImageError(e.to_string()))?;

    Ok(img.dimensions())
}

/// Get image from clipboard
///
/// Returns Ok(Some((bytes, format))) if an image was found,
/// Ok(None) if no image was in the clipboard,
/// or Err if there was an error accessing the clipboard.
fn get_clipboard_image() -> Result<Option<(Vec<u8>, ImageFormat)>, AttachmentError> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|e| AttachmentError::ClipboardError(e.to_string()))?;

    // Try to get image data from clipboard
    let image_data = match clipboard.get_image() {
        Ok(data) => data,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(e) => return Err(AttachmentError::ClipboardError(e.to_string())),
    };

    // Convert to PNG bytes
    let bytes = encode_image_to_png(&image_data)?;

    Ok(Some((bytes, ImageFormat::Png)))
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

/// Read file content based on type
fn read_file_content(
    path: &Path,
    file_type: &AttachmentType,
) -> Result<AttachmentContent, AttachmentError> {
    match file_type {
        AttachmentType::Image { .. } => {
            // Read and encode as base64
            let bytes = fs::read(path).map_err(|e| AttachmentError::IoError(e.to_string()))?;
            let encoded = base64_encode(&bytes);
            Ok(AttachmentContent::Base64(encoded))
        }
        AttachmentType::Text { language } => {
            // Read as text with optional language annotation
            let content =
                fs::read_to_string(path).map_err(|e| AttachmentError::IoError(e.to_string()))?;
            let formatted = process_code_file(&content, language.as_deref());
            Ok(AttachmentContent::Text(formatted))
        }
        AttachmentType::Document { format } => process_document(path, format),
        AttachmentType::Data { format } => process_data_file(path, format),
    }
}

/// Process a code file with language detection
fn process_code_file(content: &str, language: Option<&str>) -> String {
    if let Some(lang) = language {
        // Wrap in markdown code block with language
        format!("```{}\n{}\n```", lang, content)
    } else {
        // Plain text, no wrapping
        content.to_string()
    }
}

/// Process a document file (PDF, markdown, etc.)
fn process_document(
    path: &Path,
    format: &DocumentFormat,
) -> Result<AttachmentContent, AttachmentError> {
    match format {
        DocumentFormat::Pdf => {
            // Try to extract text from PDF using pdftotext if available
            extract_pdf_text(path)
        }
        DocumentFormat::Markdown | DocumentFormat::PlainText | DocumentFormat::RestructuredText => {
            // Read as plain text
            let content =
                fs::read_to_string(path).map_err(|e| AttachmentError::IoError(e.to_string()))?;
            Ok(AttachmentContent::Text(content))
        }
    }
}

/// Extract text from a PDF file
fn extract_pdf_text(path: &Path) -> Result<AttachmentContent, AttachmentError> {
    use std::process::Command;

    // Try pdftotext first (from poppler-utils)
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if text.trim().is_empty() {
                Ok(AttachmentContent::Text(
                    "[PDF file - no extractable text content]".to_string(),
                ))
            } else {
                Ok(AttachmentContent::Text(format!(
                    "[PDF content extracted with pdftotext]\n\n{}",
                    text
                )))
            }
        }
        _ => {
            // pdftotext not available or failed
            // Return a placeholder message
            Ok(AttachmentContent::Text(
                "[PDF file - install poppler-utils for text extraction]".to_string(),
            ))
        }
    }
}

/// Process a data file (JSON, YAML, TOML, XML)
fn process_data_file(
    path: &Path,
    format: &DataFormat,
) -> Result<AttachmentContent, AttachmentError> {
    let content = fs::read_to_string(path).map_err(|e| AttachmentError::IoError(e.to_string()))?;

    match format {
        DataFormat::Json => {
            // Try to parse and pretty-print JSON
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(value) => {
                    let pretty =
                        serde_json::to_string_pretty(&value).unwrap_or_else(|_| content.clone());
                    Ok(AttachmentContent::Text(format!("```json\n{}\n```", pretty)))
                }
                Err(_) => {
                    // Invalid JSON, return as-is with warning
                    Ok(AttachmentContent::Text(format!(
                        "[Warning: Invalid JSON]\n```json\n{}\n```",
                        content
                    )))
                }
            }
        }
        DataFormat::Yaml => {
            // Return YAML with syntax highlighting
            Ok(AttachmentContent::Text(format!(
                "```yaml\n{}\n```",
                content
            )))
        }
        DataFormat::Toml => {
            // Return TOML with syntax highlighting
            Ok(AttachmentContent::Text(format!(
                "```toml\n{}\n```",
                content
            )))
        }
        DataFormat::Xml => {
            // Return XML with syntax highlighting
            Ok(AttachmentContent::Text(format!("```xml\n{}\n```", content)))
        }
    }
}

/// Encode bytes as base64
pub fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = Vec::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;

        let n = (b0 << 16) | (b1 << 8) | b2;

        result.push(ALPHABET[((n >> 18) & 0x3F) as usize]);
        result.push(ALPHABET[((n >> 12) & 0x3F) as usize]);

        if chunk.len() > 1 {
            result.push(ALPHABET[((n >> 6) & 0x3F) as usize]);
        } else {
            result.push(b'=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[(n & 0x3F) as usize]);
        } else {
            result.push(b'=');
        }
    }

    String::from_utf8(result).unwrap_or_default()
}

/// Decode base64 to bytes
pub fn base64_decode(encoded: &str) -> Result<Vec<u8>, AttachmentError> {
    const DECODE_TABLE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62, -1, -1,
        -1, 63, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, -1, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let encoded = encoded.trim();
    if encoded.is_empty() {
        return Ok(Vec::new());
    }

    let mut result = Vec::with_capacity(encoded.len() * 3 / 4);
    let bytes = encoded.as_bytes();

    for chunk in bytes.chunks(4) {
        if chunk.len() < 4 {
            return Err(AttachmentError::IoError(
                "Invalid base64 length".to_string(),
            ));
        }

        let mut n = 0u32;
        let mut padding = 0;

        for (i, &b) in chunk.iter().enumerate() {
            if b == b'=' {
                padding += 1;
                continue;
            }

            if b >= 128 {
                return Err(AttachmentError::IoError(
                    "Invalid base64 character".to_string(),
                ));
            }

            let val = DECODE_TABLE[b as usize];
            if val < 0 {
                return Err(AttachmentError::IoError(
                    "Invalid base64 character".to_string(),
                ));
            }

            n |= (val as u32) << (18 - i * 6);
        }

        result.push((n >> 16) as u8);
        if padding < 2 {
            result.push((n >> 8) as u8);
        }
        if padding < 1 {
            result.push(n as u8);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_attachment_config_default() {
        let config = AttachmentConfig::default();
        assert_eq!(config.max_attachment_size, 10 * 1024 * 1024);
        assert_eq!(config.max_attachments, 10);
    }

    #[test]
    fn test_attachment_manager_new() {
        let manager = AttachmentManager::default();
        assert!(manager.is_empty());
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_attachment_manager_add() {
        let mut manager = AttachmentManager::default();
        let attachment = Attachment::new(
            "test.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("test content".to_string()),
        );

        assert!(manager.add(attachment).is_ok());
        assert_eq!(manager.count(), 1);
        assert!(!manager.is_empty());
    }

    #[test]
    fn test_attachment_manager_remove() {
        let mut manager = AttachmentManager::default();
        let attachment = Attachment::new(
            "test.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("test content".to_string()),
        );
        let id = attachment.id;

        manager.add(attachment).unwrap();
        assert_eq!(manager.count(), 1);

        let removed = manager.remove(id);
        assert!(removed.is_some());
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_attachment_manager_clear() {
        let mut manager = AttachmentManager::default();

        for i in 0..3 {
            let attachment = Attachment::new(
                format!("test{}.txt", i),
                AttachmentType::Text { language: None },
                100,
                AttachmentContent::Text("test".to_string()),
            );
            manager.add(attachment).unwrap();
        }

        assert_eq!(manager.count(), 3);
        manager.clear();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_attachment_manager_size_limit() {
        let config = AttachmentConfig {
            max_attachment_size: 1000,
            max_attachments: 10,
            temp_dir: std::env::temp_dir(),
        };
        let mut manager = AttachmentManager::new(config);

        let attachment = Attachment::new(
            "large.txt".to_string(),
            AttachmentType::Text { language: None },
            2000, // Exceeds limit
            AttachmentContent::Text("x".repeat(2000)),
        );

        let result = manager.add(attachment);
        assert!(matches!(result, Err(AttachmentError::FileTooLarge { .. })));
    }

    #[test]
    fn test_attachment_manager_count_limit() {
        let config = AttachmentConfig {
            max_attachment_size: 10 * 1024 * 1024,
            max_attachments: 2,
            temp_dir: std::env::temp_dir(),
        };
        let mut manager = AttachmentManager::new(config);

        // Add two attachments (should succeed)
        for i in 0..2 {
            let attachment = Attachment::new(
                format!("test{}.txt", i),
                AttachmentType::Text { language: None },
                100,
                AttachmentContent::Text("test".to_string()),
            );
            assert!(manager.add(attachment).is_ok());
        }

        // Third should fail
        let attachment = Attachment::new(
            "test3.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("test".to_string()),
        );
        let result = manager.add(attachment);
        assert!(matches!(
            result,
            Err(AttachmentError::TooManyAttachments { .. })
        ));
    }

    #[test]
    fn test_attachment_manager_take_all() {
        let mut manager = AttachmentManager::default();

        for i in 0..3 {
            let attachment = Attachment::new(
                format!("test{}.txt", i),
                AttachmentType::Text { language: None },
                100,
                AttachmentContent::Text("test".to_string()),
            );
            manager.add(attachment).unwrap();
        }

        let taken = manager.take_all();
        assert_eq!(taken.len(), 3);
        assert!(manager.is_empty());
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(1536), "1.5KB");
        assert_eq!(format_size(1024 * 1024), "1.0MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0GB");
    }

    #[test]
    fn test_image_format_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("PNG"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("jpg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("gif"), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::from_extension("webp"), Some(ImageFormat::WebP));
        assert_eq!(ImageFormat::from_extension("txt"), None);
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("rs"), Some("rust".to_string()));
        assert_eq!(detect_language("py"), Some("python".to_string()));
        assert_eq!(detect_language("js"), Some("javascript".to_string()));
        assert_eq!(detect_language("ts"), Some("typescript".to_string()));
        assert_eq!(detect_language("go"), Some("go".to_string()));
        assert_eq!(detect_language("unknown"), None);
    }

    #[test]
    fn test_detect_file_type_text() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.rs", b"fn main() {}");

        let file_type = detect_file_type(&path).unwrap();
        assert!(
            matches!(file_type, AttachmentType::Text { language: Some(lang) } if lang == "rust")
        );
    }

    #[test]
    fn test_detect_file_type_json() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.json", b"{}");

        let file_type = detect_file_type(&path).unwrap();
        assert!(matches!(
            file_type,
            AttachmentType::Data {
                format: DataFormat::Json
            }
        ));
    }

    #[test]
    fn test_detect_file_type_markdown() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.md", b"# Hello");

        let file_type = detect_file_type(&path).unwrap();
        assert!(matches!(
            file_type,
            AttachmentType::Document {
                format: DocumentFormat::Markdown
            }
        ));
    }

    #[test]
    fn test_base64_encode_decode_roundtrip() {
        let original = b"Hello, World!";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base64_encode_decode_empty() {
        let original = b"";
        let encoded = base64_encode(original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_base64_encode_decode_binary() {
        let original: Vec<u8> = (0..=255).collect();
        let encoded = base64_encode(&original);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_attachment_preview() {
        let attachment = Attachment::new(
            "test.txt".to_string(),
            AttachmentType::Text { language: None },
            1024,
            AttachmentContent::Text("test".to_string()),
        );

        let preview = attachment.preview();
        assert!(preview.contains("test.txt"));
        assert!(preview.contains("1.0KB"));
        assert!(preview.contains("📄"));
    }

    #[test]
    fn test_attachment_type_icon() {
        assert_eq!(
            AttachmentType::Image {
                format: ImageFormat::Png,
                width: 100,
                height: 100
            }
            .icon(),
            "📷"
        );
        assert_eq!(AttachmentType::Text { language: None }.icon(), "📄");
        assert_eq!(
            AttachmentType::Document {
                format: DocumentFormat::Pdf
            }
            .icon(),
            "📕"
        );
        assert_eq!(
            AttachmentType::Data {
                format: DataFormat::Json
            }
            .icon(),
            "📊"
        );
    }

    #[test]
    fn test_attach_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.txt", b"Hello, World!");

        let mut manager = AttachmentManager::default();
        let attachment = manager.attach_file(&path).unwrap();

        assert_eq!(attachment.filename, "test.txt");
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn test_attach_file_not_found() {
        let mut manager = AttachmentManager::default();
        let result = manager.attach_file(Path::new("/nonexistent/file.txt"));

        assert!(matches!(result, Err(AttachmentError::FileNotFound(_))));
    }

    #[test]
    fn test_attachment_content_size() {
        let text_content = AttachmentContent::Text("Hello".to_string());
        assert_eq!(text_content.size(), 5);

        let base64_content = AttachmentContent::Base64("SGVsbG8=".to_string());
        assert_eq!(base64_content.size(), 8);
    }

    #[test]
    fn test_total_size() {
        let mut manager = AttachmentManager::default();

        let a1 = Attachment::new(
            "a.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("x".repeat(100)),
        );
        let a2 = Attachment::new(
            "b.txt".to_string(),
            AttachmentType::Text { language: None },
            200,
            AttachmentContent::Text("x".repeat(200)),
        );

        manager.add(a1).unwrap();
        manager.add(a2).unwrap();

        assert_eq!(manager.total_size(), 300);
    }

    #[test]
    fn test_get_image_dimensions_from_bytes() {
        // Create a minimal 2x2 PNG image
        use image::{ImageBuffer, Rgba};
        use std::io::Cursor;

        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(2, 2, |_, _| Rgba([255, 0, 0, 255]));

        let mut bytes = Cursor::new(Vec::new());
        img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let png_bytes = bytes.into_inner();

        let (width, height) = get_image_dimensions_from_bytes(&png_bytes).unwrap();
        assert_eq!(width, 2);
        assert_eq!(height, 2);
    }

    #[test]
    fn test_attach_image_file() {
        use image::{ImageBuffer, Rgba};

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.png");

        // Create a test PNG image
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(10, 10, |_, _| Rgba([255, 0, 0, 255]));
        img.save(&path).unwrap();

        let mut manager = AttachmentManager::default();
        let attachment = manager.attach_file(&path).unwrap();

        assert_eq!(attachment.filename, "test.png");
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

    #[test]
    fn test_image_base64_roundtrip() {
        use image::{ImageBuffer, Rgba};
        use std::io::Cursor;

        // Create a test image
        let original_img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(4, 4, |x, y| {
            Rgba([(x * 64) as u8, (y * 64) as u8, 128, 255])
        });

        // Encode to PNG bytes
        let mut bytes = Cursor::new(Vec::new());
        original_img
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let original_bytes = bytes.into_inner();

        // Encode to base64
        let encoded = base64_encode(&original_bytes);

        // Decode from base64
        let decoded_bytes = base64_decode(&encoded).unwrap();

        // Verify bytes match
        assert_eq!(original_bytes, decoded_bytes);

        // Verify we can load the decoded bytes as an image
        let decoded_img = image::load_from_memory(&decoded_bytes).unwrap();
        assert_eq!(decoded_img.width(), 4);
        assert_eq!(decoded_img.height(), 4);
    }

    #[test]
    fn test_encode_image_to_png() {
        // Create mock image data (RGBA format)
        let width = 2;
        let height = 2;
        let bytes: Vec<u8> = vec![
            255, 0, 0, 255, // Red pixel
            0, 255, 0, 255, // Green pixel
            0, 0, 255, 255, // Blue pixel
            255, 255, 0, 255, // Yellow pixel
        ];

        let image_data = arboard::ImageData {
            width,
            height,
            bytes: std::borrow::Cow::Owned(bytes),
        };

        let png_bytes = encode_image_to_png(&image_data).unwrap();

        // Verify we can load the PNG
        let img = image::load_from_memory(&png_bytes).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
    }

    // Note: Clipboard tests are difficult to run in CI environments
    // as they require a display server. The following test is marked
    // as ignored by default.
    #[test]
    #[ignore]
    fn test_clipboard_image_capture() {
        // This test requires a display server and clipboard access
        // It's ignored by default but can be run manually
        let mut manager = AttachmentManager::default();

        // This will return None if no image is in clipboard
        let result = manager.attach_clipboard();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_file_references_simple() {
        let input = "Check this file @test.txt please";
        let refs = parse_file_references(input);
        assert_eq!(refs, vec!["test.txt"]);
    }

    #[test]
    fn test_parse_file_references_multiple() {
        let input = "@file1.txt and @file2.rs are important";
        let refs = parse_file_references(input);
        assert_eq!(refs, vec!["file1.txt", "file2.rs"]);
    }

    #[test]
    fn test_parse_file_references_quoted() {
        let input = r#"Check @"path with spaces/file.txt" please"#;
        let refs = parse_file_references(input);
        assert_eq!(refs, vec!["path with spaces/file.txt"]);
    }

    #[test]
    fn test_parse_file_references_single_quoted() {
        let input = "Check @'another path/file.txt' please";
        let refs = parse_file_references(input);
        assert_eq!(refs, vec!["another path/file.txt"]);
    }

    #[test]
    fn test_parse_file_references_mixed() {
        let input = r#"@simple.txt and @"quoted path.txt" and @'single quoted.txt'"#;
        let refs = parse_file_references(input);
        assert_eq!(
            refs,
            vec!["simple.txt", "quoted path.txt", "single quoted.txt"]
        );
    }

    #[test]
    fn test_parse_file_references_none() {
        let input = "No file references here";
        let refs = parse_file_references(input);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_parse_file_references_at_start() {
        let input = "@start.txt is at the beginning";
        let refs = parse_file_references(input);
        assert_eq!(refs, vec!["start.txt"]);
    }

    #[test]
    fn test_parse_file_references_email_not_matched() {
        // @ in the middle of a word (like email) should not be matched
        let input = "Contact user@example.com for help";
        let refs = parse_file_references(input);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_remove_file_references() {
        let input = "Check @test.txt please";
        let result = remove_file_references(input);
        assert_eq!(result, "Check please");
    }

    #[test]
    fn test_remove_file_references_multiple() {
        let input = "@file1.txt and @file2.rs are important";
        let result = remove_file_references(input);
        assert_eq!(result, "and are important");
    }

    #[test]
    fn test_remove_file_references_quoted() {
        let input = r#"Check @"path with spaces/file.txt" please"#;
        let result = remove_file_references(input);
        assert_eq!(result, "Check please");
    }

    #[test]
    fn test_resolve_file_path_relative() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        // Change to temp dir and resolve relative path
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let resolved = resolve_file_path("test.txt").unwrap();
        assert_eq!(resolved, file_path.canonicalize().unwrap());

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_resolve_file_path_absolute() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let resolved = resolve_file_path(file_path.to_str().unwrap()).unwrap();
        assert_eq!(resolved, file_path.canonicalize().unwrap());
    }

    #[test]
    fn test_resolve_file_path_not_found() {
        let result = resolve_file_path("/nonexistent/path/file.txt");
        assert!(matches!(result, Err(AttachmentError::FileNotFound(_))));
    }

    #[test]
    fn test_resolve_file_path_empty() {
        let result = resolve_file_path("");
        assert!(matches!(result, Err(AttachmentError::FileNotFound(_))));
    }

    #[test]
    fn test_attach_and_resolve_same_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        // Test that /attach and @filepath resolve to the same file
        let resolved_attach = resolve_file_path(file_path.to_str().unwrap()).unwrap();
        let resolved_at = resolve_file_path(file_path.to_str().unwrap()).unwrap();

        assert_eq!(resolved_attach, resolved_at);

        // Both should create equivalent attachments
        let mut manager1 = AttachmentManager::default();
        let mut manager2 = AttachmentManager::default();

        let attachment1 = manager1.attach_file(&resolved_attach).unwrap();
        let attachment2 = manager2.attach_file(&resolved_at).unwrap();

        assert_eq!(attachment1.filename, attachment2.filename);
        assert_eq!(attachment1.size, attachment2.size);
        assert_eq!(attachment1.file_type, attachment2.file_type);
    }

    #[test]
    fn test_process_code_file_with_language() {
        let content = "fn main() { println!(\"Hello\"); }";
        let result = process_code_file(content, Some("rust"));
        assert!(result.starts_with("```rust\n"));
        assert!(result.ends_with("\n```"));
        assert!(result.contains(content));
    }

    #[test]
    fn test_process_code_file_without_language() {
        let content = "Some plain text content";
        let result = process_code_file(content, None);
        assert_eq!(result, content);
    }

    #[test]
    fn test_process_json_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.json", br#"{"key":"value","num":42}"#);

        let result = process_data_file(&path, &DataFormat::Json).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("```json"));
            assert!(text.contains("\"key\""));
            assert!(text.contains("\"value\""));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_invalid_json_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "invalid.json", b"{ invalid json }");

        let result = process_data_file(&path, &DataFormat::Json).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("Warning: Invalid JSON"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_yaml_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(
            &dir,
            "test.yaml",
            b"key: value\nlist:\n  - item1\n  - item2",
        );

        let result = process_data_file(&path, &DataFormat::Yaml).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("```yaml"));
            assert!(text.contains("key: value"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_toml_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.toml", b"[section]\nkey = \"value\"");

        let result = process_data_file(&path, &DataFormat::Toml).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("```toml"));
            assert!(text.contains("[section]"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_xml_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.xml", b"<root><item>value</item></root>");

        let result = process_data_file(&path, &DataFormat::Xml).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("```xml"));
            assert!(text.contains("<root>"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_markdown_document() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.md", b"# Heading\n\nSome content");

        let result = process_document(&path, &DocumentFormat::Markdown).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert!(text.contains("# Heading"));
            assert!(text.contains("Some content"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_process_plain_text_document() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "test.txt", b"Plain text content here");

        let result = process_document(&path, &DocumentFormat::PlainText).unwrap();
        if let AttachmentContent::Text(text) = result {
            assert_eq!(text, "Plain text content here");
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_attach_rust_file_with_code_block() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "main.rs", b"fn main() {\n    println!(\"Hello\");\n}");

        let mut manager = AttachmentManager::default();
        let attachment = manager.attach_file(&path).unwrap();

        assert_eq!(attachment.filename, "main.rs");
        if let AttachmentContent::Text(text) = &attachment.content {
            assert!(text.contains("```rust"));
            assert!(text.contains("fn main()"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_attach_json_file_pretty_printed() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "data.json", br#"{"a":1,"b":2}"#);

        let mut manager = AttachmentManager::default();
        let attachment = manager.attach_file(&path).unwrap();

        assert_eq!(attachment.filename, "data.json");
        if let AttachmentContent::Text(text) = &attachment.content {
            assert!(text.contains("```json"));
            // Pretty printed JSON should have newlines
            assert!(text.contains("\n"));
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_message_attachment_from_text() {
        let attachment = Attachment::new(
            "test.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("Hello, World!".to_string()),
        );

        let msg_attachment = attachment.to_message_attachment();
        assert_eq!(msg_attachment.filename, "test.txt");
        assert_eq!(msg_attachment.mime_type, "text/plain");
        assert!(!msg_attachment.is_image());
        assert_eq!(msg_attachment.as_text(), Some("Hello, World!"));
        assert!(msg_attachment.as_base64().is_none());
    }

    #[test]
    fn test_message_attachment_from_image() {
        let attachment = Attachment::new(
            "image.png".to_string(),
            AttachmentType::Image {
                format: ImageFormat::Png,
                width: 100,
                height: 100,
            },
            1024,
            AttachmentContent::Base64("SGVsbG8=".to_string()),
        );

        let msg_attachment = attachment.to_message_attachment();
        assert_eq!(msg_attachment.filename, "image.png");
        assert_eq!(msg_attachment.mime_type, "image/png");
        assert!(msg_attachment.is_image());
        assert!(msg_attachment.as_text().is_none());
        assert_eq!(msg_attachment.as_base64(), Some("SGVsbG8="));
    }

    #[test]
    fn test_prepare_for_send() {
        let mut manager = AttachmentManager::default();

        let a1 = Attachment::new(
            "file1.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("content1".to_string()),
        );
        let a2 = Attachment::new(
            "file2.txt".to_string(),
            AttachmentType::Text { language: None },
            200,
            AttachmentContent::Text("content2".to_string()),
        );

        manager.add(a1).unwrap();
        manager.add(a2).unwrap();
        assert_eq!(manager.count(), 2);

        let msg_attachments = manager.prepare_for_send();
        assert_eq!(msg_attachments.len(), 2);
        assert_eq!(msg_attachments[0].filename, "file1.txt");
        assert_eq!(msg_attachments[1].filename, "file2.txt");

        // Manager should be empty after prepare_for_send
        assert!(manager.is_empty());
    }

    #[test]
    fn test_has_images() {
        let mut manager = AttachmentManager::default();

        // No images initially
        assert!(!manager.has_images());

        // Add text file
        let text = Attachment::new(
            "file.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("content".to_string()),
        );
        manager.add(text).unwrap();
        assert!(!manager.has_images());

        // Add image
        let image = Attachment::new(
            "image.png".to_string(),
            AttachmentType::Image {
                format: ImageFormat::Png,
                width: 100,
                height: 100,
            },
            1024,
            AttachmentContent::Base64("data".to_string()),
        );
        manager.add(image).unwrap();
        assert!(manager.has_images());
    }

    #[test]
    fn test_prepare_for_send_clears_attachments() {
        let mut manager = AttachmentManager::default();

        let attachment = Attachment::new(
            "test.txt".to_string(),
            AttachmentType::Text { language: None },
            100,
            AttachmentContent::Text("test".to_string()),
        );
        manager.add(attachment).unwrap();

        // First call returns attachments
        let first = manager.prepare_for_send();
        assert_eq!(first.len(), 1);

        // Second call returns empty (attachments were cleared)
        let second = manager.prepare_for_send();
        assert!(second.is_empty());
    }
}

/// Property-based tests for attachment system
///
/// **Property 3: Image Attachment Encoding**
/// **Property 4: Attachment Preview Completeness**
/// **Property 5: File Attachment Resolution**
/// **Property 6: Attachment Size Enforcement**
/// **Validates: Requirements 4.1, 4.3, 4.5, 4.2, 6.2, 5.1, 5.2, 5.7, 11.2**
/// Search for files in workspace with optional gitignore support
///
/// Returns a list of file paths matching the pattern.
/// Supports fuzzy matching on file names and paths.
///
/// # Arguments
/// * `workspace` - Root directory to search from
/// * `pattern` - Search pattern (empty string returns all files)
/// * `respect_gitignore` - Whether to respect .gitignore rules
pub fn search_workspace_files(
    workspace: &Path,
    pattern: &str,
    respect_gitignore: bool,
) -> Vec<PathBuf> {
    use ignore::WalkBuilder;

    let mut results = Vec::new();
    let pattern_lower = pattern.to_lowercase();

    let walker = WalkBuilder::new(workspace)
        .git_ignore(respect_gitignore)
        .hidden(true) // Show hidden files
        .max_depth(Some(10)) // Limit recursion depth
        .build();

    fn fuzzy_match(haystack: &str, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let mut it = haystack.chars();
        for ch in needle.chars() {
            if !it.by_ref().any(|c| c == ch) {
                return false;
            }
        }
        true
    }

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();

        // Apply filter
        if pattern.is_empty() {
            results.push(path.to_path_buf());
        } else {
            let path_str = path.to_string_lossy().to_lowercase();
            if fuzzy_match(&path_str, &pattern_lower) {
                results.push(path.to_path_buf());
            }
        }

        // Limit results to prevent overwhelming the UI
        if results.len() >= 100 {
            break;
        }
    }

    results
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    // **Feature: terminal-tui-chat, Property 3: Image Attachment Encoding**
    // **Validates: Requirements 4.1, 4.3, 4.5**
    //
    // For any valid image bytes, encoding to base64 and decoding back
    // SHALL produce the original bytes.
    proptest! {
        #[test]
        fn prop_base64_roundtrip(bytes in prop::collection::vec(any::<u8>(), 0..1000)) {
            let encoded = base64_encode(&bytes);
            let decoded = base64_decode(&encoded).unwrap();
            prop_assert_eq!(bytes, decoded);
        }
    }

    // **Feature: terminal-tui-chat, Property 3: Image Attachment Encoding**
    // **Validates: Requirements 4.1, 4.3, 4.5**
    //
    // For any valid PNG image dimensions, creating an image, encoding to base64,
    // and decoding SHALL preserve the image data.
    proptest! {
        #[test]
        fn prop_image_encoding_roundtrip(
            width in 1u32..50u32,
            height in 1u32..50u32,
            r in 0u8..=255u8,
            g in 0u8..=255u8,
            b in 0u8..=255u8,
        ) {
            use image::{ImageBuffer, Rgba};
            use std::io::Cursor;

            // Create a solid color image
            let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |_, _| {
                Rgba([r, g, b, 255])
            });

            // Encode to PNG bytes
            let mut bytes = Cursor::new(Vec::new());
            img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
            let original_bytes = bytes.into_inner();

            // Encode to base64
            let encoded = base64_encode(&original_bytes);

            // Decode from base64
            let decoded_bytes = base64_decode(&encoded).unwrap();

            // Verify bytes match
            prop_assert_eq!(&original_bytes, &decoded_bytes);

            // Verify we can load the decoded bytes as an image
            let decoded_img = image::load_from_memory(&decoded_bytes).unwrap();
            prop_assert_eq!(decoded_img.width(), width);
            prop_assert_eq!(decoded_img.height(), height);
        }
    }

    // **Feature: terminal-tui-chat, Property 4: Attachment Preview Completeness**
    // **Validates: Requirements 4.2, 6.2**
    //
    // For any attachment, the preview SHALL contain filename, type indicator, and size.
    proptest! {
        #[test]
        fn prop_attachment_preview_completeness(
            filename in "[a-zA-Z0-9_-]{1,20}\\.[a-z]{1,4}",
            size in 1u64..10_000_000u64,
        ) {
            let attachment = Attachment::new(
                filename.clone(),
                AttachmentType::Text { language: None },
                size,
                AttachmentContent::Text("test".to_string()),
            );

            let preview = attachment.preview();

            // Preview must contain filename
            prop_assert!(preview.contains(&filename),
                "Preview '{}' should contain filename '{}'", preview, filename);

            // Preview must contain size (formatted)
            let size_str = format_size(size);
            prop_assert!(preview.contains(&size_str),
                "Preview '{}' should contain size '{}'", preview, size_str);

            // Preview must contain type icon
            let icon = attachment.file_type.icon();
            prop_assert!(preview.contains(icon),
                "Preview '{}' should contain icon '{}'", preview, icon);
        }
    }

    // **Feature: terminal-tui-chat, Property 4: Attachment Preview Completeness**
    // **Validates: Requirements 4.2, 6.2**
    //
    // For any image attachment, the preview SHALL indicate it's an image.
    proptest! {
        #[test]
        fn prop_image_preview_has_image_icon(
            filename in "[a-zA-Z0-9_-]{1,20}\\.png",
            width in 1u32..1000u32,
            height in 1u32..1000u32,
            size in 1u64..10_000_000u64,
        ) {
            let attachment = Attachment::new(
                filename.clone(),
                AttachmentType::Image {
                    format: ImageFormat::Png,
                    width,
                    height,
                },
                size,
                AttachmentContent::Base64("test".to_string()),
            );

            let preview = attachment.preview();

            // Image preview must contain camera icon
            prop_assert!(preview.contains("📷"),
                "Image preview '{}' should contain camera icon", preview);
        }
    }

    // **Feature: terminal-tui-chat, Property 5: File Attachment Resolution**
    // **Validates: Requirements 5.1, 5.2**
    //
    // For any valid file path, /attach and @filepath syntax SHALL resolve
    // to the same file and create equivalent attachments.
    proptest! {
        #[test]
        fn prop_file_resolution_equivalence(
            filename in "[a-zA-Z0-9_-]{1,20}\\.txt",
            content in "[a-zA-Z0-9 ]{1,100}",
        ) {
            let dir = TempDir::new().unwrap();
            let file_path = dir.path().join(&filename);
            std::fs::write(&file_path, &content).unwrap();

            // Resolve using absolute path (like /attach)
            let resolved1 = resolve_file_path(file_path.to_str().unwrap()).unwrap();

            // Resolve using the same path (like @filepath)
            let resolved2 = resolve_file_path(file_path.to_str().unwrap()).unwrap();

            // Both should resolve to the same canonical path
            prop_assert_eq!(resolved1.clone(), resolved2.clone());

            // Both should create equivalent attachments
            let mut manager1 = AttachmentManager::default();
            let mut manager2 = AttachmentManager::default();

            let attachment1 = manager1.attach_file(&resolved1).unwrap();
            let attachment2 = manager2.attach_file(&resolved2).unwrap();

            prop_assert_eq!(attachment1.filename, attachment2.filename);
            prop_assert_eq!(attachment1.size, attachment2.size);
        }
    }

    // **Feature: terminal-tui-chat, Property 6: Attachment Size Enforcement**
    // **Validates: Requirements 5.7, 11.2**
    //
    // For any file larger than max_attachment_size, the attachment operation
    // SHALL be rejected with an appropriate error.
    proptest! {
        #[test]
        fn prop_size_limit_enforcement(
            max_size in 100u64..10000u64,
            file_size in 100u64..20000u64,
        ) {
            let config = AttachmentConfig {
                max_attachment_size: max_size,
                max_attachments: 10,
                temp_dir: std::env::temp_dir(),
            };
            let mut manager = AttachmentManager::new(config);

            let attachment = Attachment::new(
                "test.txt".to_string(),
                AttachmentType::Text { language: None },
                file_size,
                AttachmentContent::Text("x".repeat(file_size as usize)),
            );

            let result = manager.add(attachment);

            if file_size > max_size {
                // Should be rejected
                prop_assert!(matches!(result, Err(AttachmentError::FileTooLarge { .. })),
                    "File of size {} should be rejected when max is {}", file_size, max_size);
            } else {
                // Should be accepted
                prop_assert!(result.is_ok(),
                    "File of size {} should be accepted when max is {}", file_size, max_size);
            }
        }
    }

    // **Feature: terminal-tui-chat, Property 6: Attachment Size Enforcement**
    // **Validates: Requirements 5.7, 11.2**
    //
    // For any number of attachments exceeding max_attachments, the operation
    // SHALL be rejected.
    proptest! {
        #[test]
        fn prop_count_limit_enforcement(
            max_count in 1usize..10usize,
            add_count in 1usize..15usize,
        ) {
            let config = AttachmentConfig {
                max_attachment_size: 10 * 1024 * 1024,
                max_attachments: max_count,
                temp_dir: std::env::temp_dir(),
            };
            let mut manager = AttachmentManager::new(config);

            let mut success_count = 0;
            let mut rejected = false;

            for i in 0..add_count {
                let attachment = Attachment::new(
                    format!("file{}.txt", i),
                    AttachmentType::Text { language: None },
                    100,
                    AttachmentContent::Text("test".to_string()),
                );

                match manager.add(attachment) {
                    Ok(_) => success_count += 1,
                    Err(AttachmentError::TooManyAttachments { .. }) => {
                        rejected = true;
                        break;
                    }
                    Err(e) => panic!("Unexpected error: {:?}", e),
                }
            }

            if add_count > max_count {
                // Should have been rejected at some point
                prop_assert!(rejected || success_count == max_count,
                    "Should reject when adding {} attachments with max {}", add_count, max_count);
            }

            // Should never exceed max_count
            prop_assert!(manager.count() <= max_count,
                "Count {} should not exceed max {}", manager.count(), max_count);
        }
    }

    // **Feature: terminal-tui-chat, Property 5: File Attachment Resolution**
    // **Validates: Requirements 5.1, 5.2**
    //
    // For any @filepath references in input, parsing SHALL extract all paths.
    proptest! {
        #[test]
        fn prop_file_reference_parsing(
            paths in prop::collection::vec("[a-zA-Z0-9_/.-]{1,30}", 1..5),
        ) {
            // Build input with @filepath references
            let input = paths.iter()
                .map(|p| format!("@{}", p))
                .collect::<Vec<_>>()
                .join(" ");

            let parsed = parse_file_references(&input);

            // Should extract all paths
            prop_assert_eq!(parsed.len(), paths.len(),
                "Should extract {} paths from '{}'", paths.len(), input);

            // Each path should be in the result
            for path in &paths {
                prop_assert!(parsed.contains(path),
                    "Parsed paths {:?} should contain '{}'", parsed, path);
            }
        }
    }
}
