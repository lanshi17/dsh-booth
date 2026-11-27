#pragma once

#include <optional>
#include <string>
#include <vector>

#include "interaction/state.hpp"
#include "subagent/acp/messages.hpp"

namespace booth {

class Editor;

/// The last committed assistant answer, or the live stream when no answer is committed yet.
std::string last_answer(const AppState& state);

/// The raw OSC 52 sequence; tmux handles this natively via set-clipboard.
std::string clipboard_sequence(const std::string& text);

/// The tmux DCS envelope used only when allow-passthrough is enabled.
std::string clipboard_passthrough_sequence(const std::string& text);

/// Writes the text to the terminal clipboard with OSC 52. Returns false for empty text.
bool copy_to_clipboard(const std::string& text);

/// Copies the last answer and updates the notice.
void copy_answer(AppState& state);

/// Reads a clipboard raster and returns it base64-encoded, or nullopt when none exists.
std::optional<acp::ImageAttachment> clipboard_image();

/// True when the native clipboard belongs to the remote host, not the terminal.
bool clipboard_is_remote();

/// Reads clipboard text (wl-paste first, then xclip), or nullopt when empty.
std::optional<std::string> clipboard_text();

/// Reads one image file path into an attachment.
std::optional<acp::ImageAttachment> clipboard_image_from_path(std::string_view path);

/// Ctrl+V: image first, then a clipboard path to an image, then text; updates the notice.
void paste_clipboard(Editor& editor, std::vector<acp::ImageAttachment>& images, AppState& state);

}  // namespace booth
