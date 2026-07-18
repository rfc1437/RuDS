use super::*;

impl BdsApp {
    pub(super) fn handle_preview_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MainWindowLoaded(window_id) => {
                self.main_window_id = window_id;
                if self.active_post_uses_embedded_preview() {
                    self.sync_embedded_preview_for_active_post()
                } else {
                    Task::none()
                }
            }
            Message::EmbeddedPreviewReady(result) => {
                match result {
                    Ok(()) => {
                        let visible = self.active_post_uses_embedded_preview();
                        if let Some(preview) = &mut self.embedded_preview {
                            preview.controller.take_staged();
                            if let Some(url) = preview.current_url.as_deref() {
                                preview.controller.navigate(url);
                            }
                            preview.controller.set_visible(visible);
                        }
                    }
                    Err(error) => {
                        self.notify(ToastLevel::Error, &error);
                    }
                }
                Task::none()
            }
            _ => unreachable!("non-preview message routed to preview handler"),
        }
    }
}
