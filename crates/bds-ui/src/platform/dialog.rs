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
