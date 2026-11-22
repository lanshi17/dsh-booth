#pragma once

#include <cstddef>
#include <filesystem>
#include <istream>
#include <string>
#include <vector>

#include "contract/error.hpp"
#include "contract/event.hpp"

namespace booth {

/// Parses dsh's session JSONL into user/assistant/thinking transcript entries.
/// Tool calls are omitted; the result keeps at most `limit` entries.
std::vector<TranscriptEntry> parse_session_jsonl(std::istream& input, std::size_t limit = 200);

/// Finds `<sessions_dir>/<id>/session.v4.jsonl.zstd`, decompresses it with the zstd CLI and
/// parses it. Returns empty when the session is not stored; a decompression failure is an error.
Result<std::vector<TranscriptEntry>> replay_session(const std::filesystem::path& sessions_dir,
                                                    const std::string& session_id);

/// Reads the persisted `session/title` event, or empty when absent.
std::string session_title(const std::filesystem::path& sessions_dir,
                          const std::string& session_id);

}  // namespace booth
