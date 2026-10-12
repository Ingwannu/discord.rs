// Discord draws a premium button's label, emoji, and price itself, and it
// opens the SKU's purchase flow, so it has no label, emoji, url, or
// custom_id to set.
use discordrs::model::SkuId;
use discordrs::request::PremiumButton;
use discordrs::PremiumButtonBuilder;

fn premium(sku_id: SkuId) {
    let _ = PremiumButtonBuilder::new(sku_id).label("Buy");
    let _ = PremiumButtonBuilder::new(sku_id).url("https://example.com");
    let mut button = PremiumButton::new(sku_id);
    button.label = Some("Buy".to_owned());
}

fn main() {}
