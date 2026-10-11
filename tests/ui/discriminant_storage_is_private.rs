// The wire value is private: build with `new` and read with `get`, so the
// 2.x tuple-struct spelling (`ActivityType(0)`, `.0`) no longer compiles.
use discordrs::ActivityType;

fn main() {
    let kind = ActivityType(0);
    let _raw = ActivityType::PLAYING.0;
    let _ = kind;
}
