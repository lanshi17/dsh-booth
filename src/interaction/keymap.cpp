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

namespace {

std::string special_sequence(Chord::Special special, int modifier) {
  const char* final = nullptr;
  switch (special) {
    case Chord::Special::Up: final = "A"; break;
    case Chord::Special::Down: final = "B"; break;
    case Chord::Special::Home: final = "H"; break;
    case Chord::Special::End: final = "F"; break;
    case Chord::Special::PageUp: return "\x1b[5~";
    case Chord::Special::PageDown: return "\x1b[6~";
    case Chord::Special::None: return {};
  }
  if (modifier == 0) return std::string("\x1b[") + final;
  return "\x1b[1;" + std::to_string(modifier) + final;
}

}  // namespace

bool matches_chord(const Chord& chord, const ftxui::Event& event) {
  const std::string& input = event.input();
  switch (chord.kind) {
    case Chord::Kind::None: return false;
    case Chord::Kind::Escape: return input == "\x1b";
    case Chord::Kind::Enter: return input == "\r" || input == "\n";
    case Chord::Kind::BackTab: return input == "\x1b[Z";
    case Chord::Kind::Special: return input == special_sequence(chord.special, 0);
    case Chord::Kind::Ctrl:
      if (chord.special != Chord::Special::None)
        return input == special_sequence(chord.special, 5);
      return input.size() == 1 &&
             static_cast<unsigned char>(input[0]) ==
                 static_cast<unsigned char>(chord.key - 'A' + 1);
    case Chord::Kind::Alt:
      if (chord.special != Chord::Special::None)
        return input == special_sequence(chord.special, 3);
      return input.size() == 2 && input[0] == '\x1b' &&
             std::toupper(static_cast<unsigned char>(input[1])) == chord.key;
    case Chord::Kind::Shift:
      if (chord.special != Chord::Special::None)
        return input == special_sequence(chord.special, 2);
      return input.size() == 1 && input[0] == chord.key;
  }
  return false;
}

std::optional<Chord> parse_chord(std::string_view text) {
  const std::string value = trim(text);
  if (value.empty()) return std::nullopt;
  const std::string lowered = lower(value);

  if (lowered == "esc" || lowered == "escape") return Chord{Chord::Kind::Escape, 0};
  if (lowered == "enter" || lowered == "return") return Chord{Chord::Kind::Enter, 0};
  if (lowered == "shift+tab" || lowered == "backtab") return Chord{Chord::Kind::BackTab, 0};

  const auto special = [](const std::string& word) -> Chord::Special {
    if (word == "up") return Chord::Special::Up;
    if (word == "down") return Chord::Special::Down;
    if (word == "pageup" || word == "page-up") return Chord::Special::PageUp;
    if (word == "pagedown" || word == "page-down") return Chord::Special::PageDown;
    if (word == "home") return Chord::Special::Home;
    if (word == "end") return Chord::Special::End;
    return Chord::Special::None;
  };
  if (const auto key = special(lowered); key != Chord::Special::None)
    return Chord{Chord::Kind::Special, 0, key};

  const auto plus = value.find('+');
  if (plus == std::string::npos) return std::nullopt;
  const std::string modifier = lower(value.substr(0, plus));
  const std::string key_text = lower(trim(value.substr(plus + 1)));
  const auto special_key = special(key_text);
  const auto kind = modifier == "ctrl" || modifier == "control" ? Chord::Kind::Ctrl
                    : modifier == "alt" || modifier == "option"  ? Chord::Kind::Alt
                    : modifier == "shift"                         ? Chord::Kind::Shift
                                                                  : Chord::Kind::None;
  if (kind == Chord::Kind::None) return std::nullopt;
  if (special_key != Chord::Special::None) return Chord{kind, 0, special_key};
  if (key_text.size() != 1) return std::nullopt;
  const char letter = static_cast<char>(std::toupper(static_cast<unsigned char>(key_text[0])));
  if (letter < 'A' || letter > 'Z') return std::nullopt;
  return Chord{kind, letter};
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
    if (action != "booth.cancel" && action != "booth.quit" && action != "booth.editor" &&
        action != "booth.history" && action != "booth.tools" && action != "booth.thinking" &&
        action != "booth.hide_tools" && action != "booth.paste_image" &&
        action != "booth.reasoning" && action != "booth.page_up" &&
        action != "booth.page_down" && action != "booth.dequeue" &&
        action != "booth.help" && action != "booth.model" &&
        action != "booth.sessions" && action != "booth.copy") {
      continue;
    }

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
    if (action == "booth.history") bindings.history = *chord;
    if (action == "booth.tools") bindings.tools = *chord;
    if (action == "booth.thinking") bindings.thinking = *chord;
    if (action == "booth.hide_tools") bindings.hide_tools = *chord;
    if (action == "booth.paste_image") bindings.paste_image = *chord;
    if (action == "booth.reasoning") bindings.reasoning = *chord;
    if (action == "booth.page_up") bindings.page_up = *chord;
    if (action == "booth.page_down") bindings.page_down = *chord;
    if (action == "booth.dequeue") bindings.dequeue = *chord;
    if (action == "booth.help") bindings.help = *chord;
    if (action == "booth.model") bindings.model = *chord;
    if (action == "booth.sessions") bindings.sessions = *chord;
    if (action == "booth.copy") bindings.copy = *chord;
  }
  return bindings;
}

}  // namespace booth
