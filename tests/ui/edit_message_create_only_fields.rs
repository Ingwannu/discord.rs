// An edit cannot carry what only a new message has: a nonce, stickers, or a
// reply reference. `EditMessage` has no such fields or setters (clearing
// stickers for the Components V2 transition is the separate, clear-only
// `clear_stickers()`).
use discordrs::model::{MessageId, StickerId};
use discordrs::request::EditMessage;

fn edit(message_id: MessageId, sticker_id: StickerId) {
    let _ = EditMessage::new().nonce("n");
    let _ = EditMessage::new().sticker_ids(vec![sticker_id]);
    let _ = EditMessage::new().reply_to(message_id);
    let mut edit = EditMessage::new();
    edit.enforce_nonce = true;
}

fn main() {}
