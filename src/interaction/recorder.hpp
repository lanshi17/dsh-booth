#pragma once

#include <cstddef>
#include <filesystem>
#include <string>

#include <ftxui/dom/elements.hpp>

namespace booth {

/// Start a screen recording at `path`. The file is JSONL: one `start` row,
/// one `frame` row per rendered screen, and one `stop` row.
bool start_recording(const std::filesystem::path& path, std::string& error);
void stop_recording();
bool recording();
std::string recording_path();
std::size_t recorded_frames();

/// Append the rendered element as one screen frame. No-op when not recording.
void record_frame(const ftxui::Element& element);

}  // namespace booth
