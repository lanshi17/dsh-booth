#pragma once

#include <filesystem>
#include <string>
#include <string_view>

namespace booth {

/// Parses `on` or `off`; absent or malformed values default to enabled.
bool parse_notifications_enabled(std::string_view value);

/// Reads `<state_dir>/notify`, defaulting to enabled when it is absent or malformed.
bool notifications_enabled(const std::filesystem::path& state_dir);

/// Persists the preference to `<state_dir>/notify`; false indicates a write failure.
bool persist_notifications_enabled(const std::filesystem::path& state_dir, bool enabled);

/// Constructs an OSC 9 notification and trailing BEL, optionally wrapped for tmux.
std::string notification_sequence(const std::string& title, const std::string& body,
                                  bool tmux);

/// Writes and flushes a terminal notification, respecting `$TMUX` passthrough.
void send_notification(const std::string& title, const std::string& body);

}  // namespace booth
