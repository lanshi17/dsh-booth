#pragma once

#include <filesystem>
#include <iosfwd>

namespace booth {

/// Replay a `/record` JSONL file. `speed` is a delay multiplier; zero prints
/// frames without waiting. The stream receives alternate-screen frames.
int play_recording(const std::filesystem::path& path, double speed, std::ostream& out,
                   std::ostream& error);

}  // namespace booth
