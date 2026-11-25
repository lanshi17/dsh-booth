#include "interaction/command_args.hpp"

#include <algorithm>
#include <cctype>

#include "interaction/commands.hpp"

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (char& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

std::string trim_left(std::string_view text) {
  const auto first = text.find_first_not_of(" \t");
  return first == std::string_view::npos ? std::string{} : std::string(text.substr(first));
}

bool known_argument_command(std::string_view command) {
  return command == "/notify" || command == "/mouse" || command == "/effort" || command == "/model" ||
         command == "/copy" || command == "/paste";
}

void append(std::vector<ArgumentHint>& out, std::string value, std::string description) {
  const bool seen = std::any_of(out.begin(), out.end(),
                                [&](const ArgumentHint& hint) { return hint.value == value; });
  if (!seen) out.push_back({std::move(value), std::move(description)});
}

}  // namespace

std::optional<ArgumentQuery> argument_query(const std::string& composer) {
  const auto first = composer.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || composer[first] != '/') return std::nullopt;
  const auto end = composer.find_first_of(" \t\r\n", first);
  if (end == std::string::npos) return std::nullopt;
  const std::string command = normalize_command(composer.substr(first, end - first));
  if (!known_argument_command(command)) return std::nullopt;
  const std::string prefix = trim_left(std::string_view(composer).substr(end));
  if (prefix.find_first_of("\r\n") != std::string::npos) return std::nullopt;
  return ArgumentQuery{command, prefix};
}

std::vector<ArgumentHint> matching_arguments(const ArgumentQuery& query, const AppState& state) {
  std::vector<ArgumentHint> all;
  if (query.command == "/notify") {
    append(all, "on", "terminal attention alerts");
    append(all, "off", "disable alerts");
  } else if (query.command == "/mouse") {
    append(all, "on", "capture wheel, clicks and hover");
    append(all, "off", "keep terminal scrollback and selection");
  } else if (query.command == "/effort") {
    append(all, "next", "cycle to the next level");
    for (const auto& choice : state.reasoning.choices) {
      append(all, choice.name, choice.value);
    }
  } else if (query.command == "/paste") {
    append(all, "raw", "paste without collapsing markers");
  } else if (query.command == "/copy") {
    append(all, "answer", "copy the last assistant answer");
    append(all, "prompt", "copy the current composer");
    append(all, "line", "copy the current composer line");
  } else if (query.command == "/model") {
    append(all, "auto", "let dsh route");
    for (const auto& choice : state.models) {
      append(all, choice.name, choice.provider + " · " + choice.value);
    }
  }
  const std::string prefix = lower(query.prefix);
  if (prefix.empty()) return all;
  std::vector<ArgumentHint> matches;
  for (auto& hint : all) {
    if (lower(hint.value).rfind(prefix, 0) == 0) matches.push_back(std::move(hint));
  }
  return matches;
}

}  // namespace booth
