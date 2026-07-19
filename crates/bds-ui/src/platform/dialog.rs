use iced::Task;

use crate::app::Message;

/// Pick a folder using the native file dialog.
pub fn pick_folder(title: String) -> Task<Message> {
    Task::perform(
        async move {
            rfd::AsyncFileDialog::new()
                .set_title(&title)
                .pick_folder()
                .await
                .map(|h| h.path().to_path_buf())
        },
        Message::FolderPicked,
    )
}

/// Pick an existing portable project folder.
pub fn pick_project_folder(title: String) -> Task<Message> {
    Task::perform(
        async move {
            rfd::AsyncFileDialog::new()
                .set_title(&title)
                .pick_folder()
                .await
                .map(|h| h.path().to_path_buf())
        },
        Message::ProjectFolderPicked,
    )
}

/// Pick one or more image/media files using the native file dialog.
pub fn pick_media_files(title: String, filter_label: String) -> Task<Message> {
    Task::perform(
        async move {
            rfd::AsyncFileDialog::new()
                .set_title(&title)
                .add_filter(
                    &filter_label,
                    &["jpg", "jpeg", "png", "gif", "webp", "svg", "tiff", "bmp"],
                )
                .pick_files()
                .await
                .map(|hs| hs.into_iter().map(|h| h.path().to_path_buf()).collect())
        },
        Message::MediaFilesPicked,
    )
}

/// Pick images for a persisted post's batch gallery workflow.
pub fn pick_gallery_images(post_id: String, title: String, filter_label: String) -> Task<Message> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                std::panic::catch_unwind(|| {
                    rfd::FileDialog::new()
                        .set_title(&title)
                        .add_filter(
                            &filter_label,
                            &["jpg", "jpeg", "png", "gif", "webp", "tiff", "bmp"],
                        )
                        .pick_files()
                })
                .map_err(|_| "native image picker failed".to_string())
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            (post_id, result)
        },
        |(post_id, result)| Message::GalleryImagesPicked { post_id, result },
    )
}

/// Pick a replacement image for an existing media item.
pub fn pick_media_replacement(
    media_id: String,
    title: String,
    filter_label: String,
) -> Task<Message> {
    Task::perform(
        async move {
            let path = rfd::AsyncFileDialog::new()
                .set_title(&title)
                .add_filter(
                    &filter_label,
                    &["jpg", "jpeg", "png", "gif", "webp", "tiff", "bmp"],
                )
                .pick_file()
                .await
                .map(|handle| handle.path().to_path_buf());
            (media_id, path)
        },
        |(media_id, path)| Message::MediaReplacementPicked { media_id, path },
    )
}
