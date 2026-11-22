#include "interaction/keymap.hpp"

#include <cctype>
#include <fstream>
#include <string>

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

bool matches_chord(const Chord& chord, const ftxui::Event& event) {
  const std::string& input = event.input();
  switch (chord.kind) {
    case Chord::Kind::None: return false;
    case Chord::Kind::Escape: return input == "\x1b";
    case Chord::Kind::Enter: return input == "\r" || input == "\n";
    case Chord::Kind::Ctrl:
      return input.size() == 1 &&
             static_cast<unsigned char>(input[0]) ==
                 static_cast<unsigned char>(chord.key - 'A' + 1);
    case Chord::Kind::Alt:
      return input.size() == 2 && input[0] == '\x1b' &&
             std::toupper(static_cast<unsigned char>(input[1])) == chord.key;
  }
  return false;
}

std::optional<Chord> parse_chord(std::string_view text) {
  const std::string value = trim(text);
  if (value.empty()) return std::nullopt;
  const std::string lowered = lower(value);

  if (lowered == "esc" || lowered == "escape") return Chord{Chord::Kind::Escape, 0};
  if (lowered == "enter" || lowered == "return") return Chord{Chord::Kind::Enter, 0};

  const auto plus = value.find('+');
  if (plus == std::string::npos) return std::nullopt;
  const std::string modifier = lower(value.substr(0, plus));
  const std::string key = trim(value.substr(plus + 1));
  if (key.size() != 1) return std::nullopt;
  const char letter = static_cast<char>(std::toupper(static_cast<unsigned char>(key[0])));
  if (letter < 'A' || letter > 'Z') return std::nullopt;

  if (modifier == "ctrl" || modifier == "control") return Chord{Chord::Kind::Ctrl, letter};
  if (modifier == "alt" || modifier == "option") return Chord{Chord::Kind::Alt, letter};
  return std::nullopt;
}

Result<KeyBindings> load_keybindings(const std::filesystem::path& path) {
  KeyBindings bindings;
  std::ifstream input(path);
  if (!input) return bindings;

  std::string line;
  std::size_t number = 0;
  while (std::getline(input, line)) {
    ++number;
    const auto comment = line.find('#');
    if (comment != std::string::npos) line.resize(comment);
    const std::string entry = trim(line);
    if (entry.empty()) continue;

    const auto colon = entry.find(':');
    if (colon == std::string::npos) {
      return std::unexpected(Error{Layer::Config,
                                   "keybindings line " + std::to_string(number) + " has no ':'",
                                   std::nullopt});
    }
    const std::string action = lower(trim(entry.substr(0, colon)));
    const std::string chord_text = trim(entry.substr(colon + 1));
    if (action != "booth.cancel" && action != "booth.quit" && action != "booth.editor") continue;

    const auto chord = parse_chord(chord_text);
    if (!chord) {
      return std::unexpected(Error{Layer::Config,
                                   "keybindings line " + std::to_string(number) +
                                       " names an unknown chord",
                                   std::nullopt});
    }
    if (action == "booth.cancel") bindings.cancel = *chord;
    if (action == "booth.quit") bindings.quit = *chord;
    if (action == "booth.editor") bindings.editor = *chord;
  }
  return bindings;
}

}  // namespace booth
