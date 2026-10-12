// The `id` of an attachment entry means a stored attachment
// (`AttachmentId`, a snowflake) when keeping one, and the `n` of the
// `files[n]` part (`UploadId`) when describing an upload. The two cannot be
// swapped.
use discordrs::model::{AttachmentId, UploadId};
use discordrs::request::{AttachmentUpload, ExistingAttachment, MessageAttachment};

fn attachments(stored: AttachmentId) {
    let _ = MessageAttachment::keep(UploadId::new(0));
    let _ = MessageAttachment::upload(stored);
    let _ = AttachmentUpload::new(stored);
    let _ = ExistingAttachment::new(UploadId::new(0));
}

fn main() {}
