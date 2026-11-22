#include "interaction/commands.hpp"

#include <algorithm>
#include <cctype>
#include <iterator>
#include <utility>

namespace booth {

constexpr CommandHint kCommandHints[] = {
    {"/help", "keys and commands", "shortcuts keys help", ""},
    {"/model", "switch the session model", "provider choose config", ""},
    {"/status", "phase, session and model", "info state session", ""},
    {"/copy", "copy the last answer", "clipboard yank", ""},
    {"/clear", "clear the local transcript", "reset erase screen", ""},
    {"/retry", "send the last prompt again", "again rerun repeat", ""},
    {"/export", "write the transcript as markdown", "save file markdown", ""},
    {"/sessions", "resume a saved session", "list history resume", ""},
    {"/close", "close the active session", "end session", ""},
    {"/quit", "leave booth", "exit", "/exit"},
};

namespace {

std::string lower_ascii(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

int subsequence_score(std::string_view query, std::string_view target) {
  if (query.size() > target.size()) return 0;
  std::size_t matched = 0;
  int gaps = 0;
  int last = -1;
  for (int index = 0; index < static_cast<int>(target.size()) && matched < query.size(); ++index) {
    if (target[static_cast<std::size_t>(index)] != query[matched]) continue;
    if (last >= 0 && index - last > 1) ++gaps;
    last = index;
    ++matched;
  }
  if (matched != query.size()) return 0;
  return std::max(1, 40 - gaps * 5);
}

int text_score(std::string_view query, std::string_view target) {
  if (query.empty()) return 1;
  if (target == query) return 100;
  if (target.rfind(query, 0) == 0) return 80;
  if (target.find(query) != std::string_view::npos) return 60;
  return subsequence_score(query, target);
}

int hint_score(const CommandHint& hint, const std::string& query) {
  if (query.empty()) return 1;
  int score = text_score(query, lower_ascii(std::string(hint.name).substr(1)));
  if (hint.alias[0] != '\0') {
    score = std::max(score, text_score(query, lower_ascii(std::string(hint.alias).substr(1))));
  }
  const std::string searchable =
      lower_ascii(std::string(hint.description) + " " + hint.keywords);
  if (searchable.find(query) != std::string::npos ||
      subsequence_score(query, searchable) > 0) {
    score = std::max(score, 20);
  }
  return score;
}

}  // namespace

std::optional<std::string> slash_query(const std::string& composer) {
  const auto first = composer.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || composer[first] != '/') return std::nullopt;
  const std::string rest = composer.substr(first + 1);
  if (rest.find_first_of(" \t\r\n") != std::string::npos) return std::nullopt;
  return lower_ascii(rest);
}

bool command_name_matches(const CommandHint& hint, std::string_view query) {
  if (query.empty()) return true;
  if (text_score(query, lower_ascii(std::string(hint.name).substr(1))) > 0) return true;
  return hint.alias[0] != '\0' &&
         text_score(query, lower_ascii(std::string(hint.alias).substr(1))) > 0;
}

std::vector<const CommandHint*> matching_commands(const std::string& composer) {
  const auto query = slash_query(composer);
  if (!query) return {};
  std::vector<std::pair<int, const CommandHint*>> ranked;
  ranked.reserve(std::size(kCommandHints));
  for (const auto& hint : kCommandHints) {
    const int score = hint_score(hint, *query);
    if (score > 0) ranked.emplace_back(score, &hint);
  }
  std::stable_sort(ranked.begin(), ranked.end(),
                   [](const auto& left, const auto& right) { return left.first > right.first; });
  std::vector<const CommandHint*> matches;
  matches.reserve(ranked.size());
  for (const auto& [score, hint] : ranked) {
    (void)score;
    matches.push_back(hint);
  }
  return matches;
}

}  // namespace booth
