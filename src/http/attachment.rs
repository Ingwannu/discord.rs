/// File data attached to a Discord multipart request.
///
/// The request body is sent with a `payload_json` part plus one `files[n]`
/// part per attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileAttachment {
    pub filename: String,
    pub data: Vec<u8>,
    pub content_type: Option<String>,
}

/// Type alias for `FileUpload`.
pub type FileUpload = FileAttachment;

impl FileAttachment {
    /// Creates a `new` value.
    pub fn new(filename: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            filename: filename.into(),
            data: data.into(),
            content_type: None,
        }
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}
