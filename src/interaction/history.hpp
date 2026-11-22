#pragma once

#include <filesystem>
#include <string>
#include <vector>

namespace booth {

/// Reads the prompt history, one `{"text": ...}` JSON object per line.
std::vector<std::string> load_history(const std::filesystem::path& path);

/// Appends one prompt and keeps the file owner-readable only.
void append_history(const std::filesystem::path& path, const std::string& text);

}  // namespace booth
