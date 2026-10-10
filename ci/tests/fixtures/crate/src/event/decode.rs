pub fn decode_event(event_name: &str, data: Value) -> Result<Event, DiscordError> {
    let event = match event_name {
        "READY" => Event::Ready(data),
        "MESSAGE_CREATE" | "MESSAGE_UPDATE" => Event::Message(data),
        "GUILD_MEMBERS_CHUNK" => Event::Chunk(data),
        _ => Event::Unknown {
            kind: "UNKNOWN".to_string(),
        },
    };
    Ok(event)
}

#[cfg(test)]
mod tests {
    fn sample() {
        let _ = match name { "TEST_ONLY_EVENT" => 1, _ => 0 };
    }
}

// Commented-out arms must not count:
// "COMMENTED_OUT_EVENT" => Event::Unknown,
