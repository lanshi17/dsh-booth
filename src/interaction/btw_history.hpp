#pragma once

#include <filesystem>
#include <string>
#include <string_view>

namespace booth {

/// Rendered questions and answers from `btw-history.jsonl`.
std::string btw_history_text(const std::filesystem::path& path, std::string_view session);

/// Append one side-question exchange for later `/btw` browsing.
void append_btw_history(const std::filesystem::path& path, std::string_view session,
                        const std::string& question, const std::string& answer);

}  // namespace booth
