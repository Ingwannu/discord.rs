//! Media gallery, thumbnail, file, and section builders. Each holds its
//! typed request model ([`MediaGallery`], [`Thumbnail`], [`FileComponent`],
//! [`Section`]).

use crate::error::DiscordError;
use crate::model::ComponentId;
use crate::request::{
    Button, Component, ContainerComponent, FileComponent, MediaGallery, Section, SectionAccessory,
    TextDisplay, Thumbnail,
};
use crate::types::MediaGalleryItem;

use super::container::TextDisplayBuilder;

/// Builds a [`MediaGallery`] (type 12) of 1-10 items.
#[derive(Clone, Debug, Default)]
pub struct MediaGalleryBuilder {
    inner: MediaGallery,
}

impl MediaGalleryBuilder {
    /// An empty gallery.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an item.
    pub fn add_item(mut self, item: MediaGalleryItem) -> Self {
        self.inner.items.push(item);
        self
    }

    /// Appends items.
    pub fn add_items(mut self, items: Vec<MediaGalleryItem>) -> Self {
        self.inner.items.extend(items);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The gallery, without checking limits.
    pub fn build(self) -> MediaGallery {
        self.inner
    }

    /// Checks the gallery: 1-10 items, descriptions up to 1024 characters.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<MediaGallery, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Builds a [`Thumbnail`] (type 11), a [`SectionBuilder`] accessory.
#[derive(Clone, Debug)]
pub struct ThumbnailBuilder {
    inner: Thumbnail,
}

impl ThumbnailBuilder {
    /// A thumbnail of the image at `url` (`https://` or
    /// `attachment://<filename>`).
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            inner: Thumbnail::new(url),
        }
    }

    /// Sets the alt text (up to 1024 characters).
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.inner.description = Some(description.into());
        self
    }

    /// Blurs the image as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.inner.spoiler = Some(spoiler);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The thumbnail, without checking limits.
    pub fn build(self) -> Thumbnail {
        self.inner
    }

    /// Checks the description length (up to 1024 characters).
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Thumbnail, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Builds a [`FileComponent`] (type 13), which shows a file uploaded with
/// the message.
#[derive(Clone, Debug)]
pub struct FileBuilder {
    inner: FileComponent,
}

impl FileBuilder {
    /// A file component for `attachment://<filename>`.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            inner: FileComponent::new(url),
        }
    }

    /// Blurs the file as a spoiler.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.inner.spoiler = Some(spoiler);
        self
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.inner.id = Some(id);
        self
    }

    /// The file component, without checking limits.
    pub fn build(self) -> FileComponent {
        self.inner
    }

    /// Checks that the URL is an `attachment://` reference.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.inner.validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<FileComponent, DiscordError> {
        self.validate()?;
        Ok(self.inner)
    }
}

/// Builds a [`Section`] (type 9): 1-3 text displays and an accessory.
///
/// Discord requires the accessory, so the builder tracks it in its type:
/// [`SectionBuilder::new`] starts as `SectionBuilder<()>`, and
/// [`set_thumbnail_accessory`](Self::set_thumbnail_accessory) or
/// [`set_button_accessory`](Self::set_button_accessory) turn it into
/// `SectionBuilder<SectionAccessory>`, the only state with `build()`. A
/// section without an accessory therefore does not compile where a section
/// is added, instead of failing at Discord.
#[derive(Clone, Debug, Default)]
pub struct SectionBuilder<A = ()> {
    id: Option<ComponentId>,
    components: Vec<TextDisplay>,
    accessory: A,
}

impl SectionBuilder<()> {
    /// A section without text or an accessory yet.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<A> SectionBuilder<A> {
    /// Appends a text display (1-3 per section).
    pub fn add_text_display(mut self, text: TextDisplayBuilder) -> Self {
        self.components.push(text.build());
        self
    }

    /// Sets a thumbnail as the accessory, replacing any earlier one.
    pub fn set_thumbnail_accessory(
        self,
        thumbnail: ThumbnailBuilder,
    ) -> SectionBuilder<SectionAccessory> {
        self.with_accessory(SectionAccessory::Thumbnail(thumbnail.build()))
    }

    /// Sets a button as the accessory, replacing any earlier one.
    pub fn set_button_accessory(
        self,
        button: impl Into<Button>,
    ) -> SectionBuilder<SectionAccessory> {
        self.with_accessory(SectionAccessory::Button(button.into()))
    }

    /// Sets the component `id`.
    pub fn id(mut self, id: ComponentId) -> Self {
        self.id = Some(id);
        self
    }

    fn with_accessory(self, accessory: SectionAccessory) -> SectionBuilder<SectionAccessory> {
        SectionBuilder {
            id: self.id,
            components: self.components,
            accessory,
        }
    }
}

impl SectionBuilder<SectionAccessory> {
    /// The section, without checking limits.
    pub fn build(self) -> Section {
        Section {
            id: self.id,
            components: self.components,
            ..Section::new(self.accessory)
        }
    }

    /// Checks the section: 1-3 text displays and a valid accessory.
    pub fn validate(&self) -> Result<(), DiscordError> {
        self.clone().build().validate()
    }

    /// [`Self::build`] after [`Self::validate`].
    pub fn try_build(self) -> Result<Section, DiscordError> {
        self.validate()?;
        Ok(self.build())
    }
}

/// `From` a builder into both top-level and container-child position.
macro_rules! builder_into_component {
    ($($builder:ty => $variant:ident),*) => {$(
        impl From<$builder> for Component {
            fn from(builder: $builder) -> Self {
                Self::$variant(builder.build())
            }
        }

        impl From<$builder> for ContainerComponent {
            fn from(builder: $builder) -> Self {
                Self::$variant(builder.build())
            }
        }
    )*};
}

builder_into_component!(
    MediaGalleryBuilder => MediaGallery,
    FileBuilder => File,
    SectionBuilder<SectionAccessory> => Section
);

impl From<ThumbnailBuilder> for SectionAccessory {
    fn from(builder: ThumbnailBuilder) -> Self {
        Self::Thumbnail(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{ButtonStyle, ComponentId};
    use serde_json::{json, Value};

    use super::{FileBuilder, MediaGalleryBuilder, SectionBuilder, ThumbnailBuilder};
    use crate::builders::{CustomButtonBuilder, TextDisplayBuilder};
    use crate::types::MediaGalleryItem;

    fn to_json<T: serde::Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("serializable")
    }

    #[test]
    fn media_gallery_builder_serializes_items_and_id() {
        let payload = MediaGalleryBuilder::new()
            .add_item(MediaGalleryItem::new("https://example.com/one.png"))
            .add_items(vec![
                MediaGalleryItem::new("https://example.com/two.png").description("second"),
                MediaGalleryItem::new("https://example.com/three.png").spoiler(true),
            ])
            .id(ComponentId::new(7))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({
                "type": 12, "id": 7, "items": [
                    {"media": {"url": "https://example.com/one.png"}},
                    {"media": {"url": "https://example.com/two.png"}, "description": "second"},
                    {"media": {"url": "https://example.com/three.png"}, "spoiler": true}
                ]
            })
        );
    }

    #[test]
    fn thumbnail_builder_serializes_optional_fields() {
        assert_eq!(
            to_json(&ThumbnailBuilder::new("https://example.com/thumb.png").build()),
            json!({"type": 11, "media": {"url": "https://example.com/thumb.png"}})
        );
        let payload = ThumbnailBuilder::new("https://example.com/thumb.png")
            .description("preview")
            .spoiler(true)
            .id(ComponentId::new(3))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({
                "type": 11, "id": 3,
                "media": {"url": "https://example.com/thumb.png"},
                "description": "preview", "spoiler": true
            })
        );
    }

    #[test]
    fn file_builder_serializes_optional_fields() {
        assert_eq!(
            to_json(&FileBuilder::new("attachment://file.txt").build()),
            json!({"type": 13, "file": {"url": "attachment://file.txt"}})
        );
        let payload = FileBuilder::new("attachment://file.txt")
            .spoiler(true)
            .id(ComponentId::new(11))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({"type": 13, "id": 11, "file": {"url": "attachment://file.txt"}, "spoiler": true})
        );
    }

    fn expect_model_error<T: std::fmt::Debug>(
        result: Result<T, crate::error::DiscordError>,
        needle: &str,
    ) {
        let err = result.expect_err("expected validation failure").to_string();
        assert!(
            err.contains(needle),
            "error message {err:?} should contain {needle:?}"
        );
    }

    #[test]
    fn media_gallery_try_build_matches_build_and_rejects_invalid_galleries() {
        let make = || {
            MediaGalleryBuilder::new().add_item(MediaGalleryItem::new("https://example.com/a.png"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid gallery"));
        expect_model_error(
            MediaGalleryBuilder::new().try_build(),
            "media gallery items must contain 1-10 items, got 0",
        );
        let mut overfull = MediaGalleryBuilder::new();
        for _ in 0..11 {
            overfull = overfull.add_item(MediaGalleryItem::new("https://example.com/a.png"));
        }
        expect_model_error(
            overfull.try_build(),
            "media gallery items must contain 1-10 items, got 11",
        );
        expect_model_error(
            MediaGalleryBuilder::new()
                .add_item(
                    MediaGalleryItem::new("https://example.com/a.png")
                        .description(&"d".repeat(1025)),
                )
                .try_build(),
            "media gallery item 0 description must be at most 1024 characters, got 1025",
        );
    }

    #[test]
    fn thumbnail_and_file_try_build_check_their_rules() {
        let make = || ThumbnailBuilder::new("https://example.com/thumb.png").description("ok");
        assert_eq!(make().build(), make().try_build().expect("valid thumbnail"));
        expect_model_error(
            ThumbnailBuilder::new("https://example.com/thumb.png")
                .description("d".repeat(1025))
                .try_build(),
            "thumbnail description must be at most 1024 characters, got 1025",
        );
        expect_model_error(
            FileBuilder::new("https://example.com/file.txt").try_build(),
            "file component url must use attachment://<filename>",
        );
    }

    #[test]
    fn section_try_build_matches_build_and_rejects_invalid_sections() {
        let make = || {
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("body"))
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
        };
        assert_eq!(make().build(), make().try_build().expect("valid section"));
        expect_model_error(
            SectionBuilder::new()
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
                .try_build(),
            "section components must contain 1-3 items, got 0",
        );
        expect_model_error(
            SectionBuilder::new()
                .add_text_display(TextDisplayBuilder::new("one"))
                .add_text_display(TextDisplayBuilder::new("two"))
                .add_text_display(TextDisplayBuilder::new("three"))
                .add_text_display(TextDisplayBuilder::new("four"))
                .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/t.png"))
                .try_build(),
            "section components must contain 1-3 items, got 4",
        );
    }

    #[test]
    fn section_builder_serializes_text_and_accessory_in_any_order() {
        let payload = SectionBuilder::new()
            .set_thumbnail_accessory(
                ThumbnailBuilder::new("https://example.com/thumb.png")
                    .description("preview")
                    .id(ComponentId::new(8)),
            )
            .add_text_display(TextDisplayBuilder::new("title"))
            .id(ComponentId::new(4))
            .build();
        assert_eq!(
            to_json(&payload),
            json!({
                "type": 9, "id": 4,
                "components": [{"type": 10, "content": "title"}],
                "accessory": {
                    "type": 11, "id": 8,
                    "media": {"url": "https://example.com/thumb.png"},
                    "description": "preview"
                }
            })
        );
    }

    #[test]
    fn section_builder_can_replace_accessory_with_button() {
        let payload = SectionBuilder::new()
            .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/thumb.png"))
            .set_button_accessory(
                CustomButtonBuilder::new("open")
                    .label("Open")
                    .style(ButtonStyle::SECONDARY),
            )
            .add_text_display(TextDisplayBuilder::new("body"))
            .build();
        assert_eq!(
            to_json(&payload)["accessory"],
            json!({"type": 2, "style": 2, "label": "Open", "custom_id": "open"})
        );
    }
}
