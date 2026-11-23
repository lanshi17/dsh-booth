#include "interaction/transcript_search.hpp"

#include <cctype>

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

}  // namespace

void TranscriptSearch::collect(const AppState& state) {
  matches_.clear();
  const std::string needle = lower(query_);
  for (std::size_t index = 0; index < state.entries.size(); ++index) {
    const auto& entry = state.entries[index];
    const std::string haystack = lower(entry.text + "\n" + entry.input + "\n" + entry.output +
                                      "\n" + entry.tool);
    if (haystack.find(needle) != std::string::npos) matches_.push_back(index);
  }
}

std::string TranscriptSearch::find(AppState& state, TranscriptScroll& scroll,
                                   const std::string& argument) {
  const std::string query = trim(argument);
  if (!query.empty() && (!has_query_ || query != query_)) {
    query_ = query;
    has_query_ = true;
    collect(state);
    position_ = matches_.empty() ? 0 : matches_.size() - 1;
  } else if (!has_query_) {
    return "usage: /find <text>";
  } else if (matches_.empty()) {
    return "no match for " + query_;
  } else {
    position_ = (position_ + matches_.size() - 1) % matches_.size();
  }

  if (matches_.empty()) return "no match for " + query_;
  const std::size_t entry = matches_[position_];
  scroll.focus(state.entries.size(), entry);
  return "match " + std::to_string(position_ + 1) + "/" + std::to_string(matches_.size()) +
         " · entry " + std::to_string(entry + 1);
}

}  // namespace booth
