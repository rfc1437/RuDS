use iced::Task;

use crate::app::Message;

/// Pick a folder using the native file dialog.
pub fn pick_folder() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Select Folder")
                .pick_folder()
                .await
                .map(|h| h.path().to_path_buf())
        },
        Message::FolderPicked,
    )
}

/// Pick one or more image/media files using the native file dialog.
pub fn pick_media_files() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Import Media")
                .add_filter(
                    "Images",
                    &["jpg", "jpeg", "png", "gif", "webp", "svg", "tiff", "bmp"],
                )
                .pick_files()
                .await
                .map(|hs| hs.into_iter().map(|h| h.path().to_path_buf()).collect())
        },
        Message::MediaFilesPicked,
    )
}
