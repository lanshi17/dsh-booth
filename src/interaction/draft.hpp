#pragma once

#include <filesystem>
#include <string>

namespace booth {

/// Reads an unsent draft; a missing or unreadable file is empty.
std::string load_draft(const std::filesystem::path& path);

/// Writes a draft with owner-only permissions; empty text removes it.
void save_draft(const std::filesystem::path& path, const std::string& text);

}  // namespace booth
