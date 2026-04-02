// macOS lifecycle shim — objc2 hooks for NSApplicationDelegate.
//
// Handles:
// - application:openFile: (Finder open)
// - application:openURLs: (URL scheme handling)
//
// These will be forwarded as Message variants into the Iced event loop
// via a channel-based subscription.
//
// Implementation deferred to M2 when the full message routing is in place.
