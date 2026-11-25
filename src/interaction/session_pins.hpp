#pragma once

#include <filesystem>
#include <set>
#include <string>

namespace booth {

/// One session id per line; a missing or unreadable file is empty.
std::set<std::string> load_session_pins(const std::filesystem::path& path);

/// Atomically replaces the pin file; false when the write failed.
bool save_session_pins(const std::filesystem::path& path, const std::set<std::string>& pins);

/// Adds or removes one id and writes the result; false when the write failed.
bool set_session_pin(const std::filesystem::path& path, const std::string& id, bool pinned);

}  // namespace booth
