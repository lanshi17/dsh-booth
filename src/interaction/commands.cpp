#include "interaction/commands.hpp"

#include <algorithm>
#include <cctype>
#include <iterator>
#include <utility>

namespace booth {

constexpr CommandHint kCommandHints[] = {
    {"/help", "keys and commands", "shortcuts keys help", "hotkeys keys"},
    {"/model", "switch the session model", "provider choose config", "models"},
    {"/status", "phase, session and model", "info state session", ""},
    {"/notify", "terminal notifications on or off", "alerts attention bell", ""},
    {"/theme", "reload or set live colors", "palette color accent", ""},
    {"/context", "context window usage", "tokens window percent usage", ""},
    {"/stats", "transcript and tool counters", "counts entries tools", ""},
    {"/copy", "copy the last answer", "clipboard yank", ""},
    {"/links", "open a transcript URL", "link url browser hint", ""},
    {"/mouse", "terminal/tmux mouse capture", "scroll select copy tmux", ""},
    {"/clear", "clear the local transcript", "reset erase screen", ""},
    {"/retry", "send the last prompt again", "again rerun repeat", ""},
    {"/export", "write the transcript as markdown", "save file markdown", ""},
    {"/sessions", "resume a saved session", "list history resume", "resume"},
    {"/new", "start a fresh session", "fresh reset close", ""},
    {"/effort", "set the reasoning effort", "reasoning thinking level", "reasoning"},
    {"/queue", "queue a prompt while a run is active", "later after busy", ""},
    {"/session", "show the active ACP session", "info id title workspace", ""},
    {"/acp", "show the ACP connection", "protocol socket phase transport", ""},
    {"/cancel", "cancel the active run", "stop abort interrupt", ""},
    {"/find", "search the transcript", "grep locate text", ""},
    {"/attach", "attach a file as an ACP resource link", "file resource prompt", ""},
    {"/detach", "clear attached resource links", "remove file resource", ""},
    {"/close", "close the active session", "end session", ""},
    {"/quit", "leave booth", "exit quit", "exit q"},
};

std::span<const CommandHint> command_hints() { return kCommandHints; }

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

template <class Fn>
void for_each_alias(const char* aliases, Fn&& fn) {
  std::string_view rest(aliases);
  while (!rest.empty()) {
    const auto space = rest.find(' ');
    const std::string_view alias = rest.substr(0, space);
    if (!alias.empty()) fn(alias);
    if (space == std::string_view::npos) break;
    rest.remove_prefix(space + 1);
  }
}

int hint_score(const CommandHint& hint, const std::string& query) {
  if (query.empty()) return 1;
  int score = text_score(query, lower_ascii(std::string(hint.name).substr(1)));
  for_each_alias(hint.aliases, [&](std::string_view alias) {
    score = std::max(score, text_score(query, lower_ascii(alias)));
  });
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
  bool matched = false;
  for_each_alias(hint.aliases, [&](std::string_view alias) {
    if (!matched && text_score(query, lower_ascii(alias)) > 0) matched = true;
  });
  return matched;
}

std::string normalize_command(std::string line) {
  const auto first = line.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || line[first] != '/') return line;
  const auto end = line.find_first_of(" \t\r\n", first);
  const std::string token = line.substr(first, end == std::string::npos ? std::string::npos : end - first);
  const std::string name = token.substr(1);
  for (const auto& hint : kCommandHints) {
    if (std::string_view(hint.name).substr(1) == name) return line;
    bool alias = false;
    for_each_alias(hint.aliases, [&](std::string_view candidate) {
      if (candidate == name) alias = true;
    });
    if (!alias) continue;
    return line.substr(0, first) + hint.name +
           (end == std::string::npos ? "" : line.substr(end));
  }
  return line;
}

std::vector<const CommandHint*> matching_commands(const std::string& composer) {
  const auto query = slash_query(composer);
  if (!query) return {};
  struct Ranked {
    int score;
    std::size_t name_length;
    std::size_t order;
    const CommandHint* hint;
  };
  std::vector<Ranked> ranked;
  ranked.reserve(std::size(kCommandHints));
  std::size_t order = 0;
  for (const auto& hint : kCommandHints) {
    const int score = hint_score(hint, *query);
    if (score > 0) {
      ranked.push_back({score, std::string_view(hint.name).size(), order++, &hint});
    }
  }
  const bool prefer_short_names = !query->empty();
  std::stable_sort(ranked.begin(), ranked.end(), [prefer_short_names](const Ranked& left, const Ranked& right) {
    if (left.score != right.score) return left.score > right.score;
    if (prefer_short_names && left.name_length != right.name_length) {
      return left.name_length < right.name_length;
    }
    return left.order < right.order;
  });
  std::vector<const CommandHint*> matches;
  matches.reserve(ranked.size());
  for (const auto& entry : ranked) matches.push_back(entry.hint);
  return matches;
}

}  // namespace booth
