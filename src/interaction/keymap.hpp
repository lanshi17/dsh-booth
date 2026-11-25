#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "contract/error.hpp"

#include <ftxui/component/event.hpp>

namespace booth {

struct Chord {
  enum class Kind { None, Ctrl, Alt, Shift, Escape, Enter, BackTab, Special };
  enum class Special { None, Up, Down, PageUp, PageDown, Home, End };
  Kind kind{Kind::None};
  char key{0};
  Special special{Special::None};
};

/// Parses `Ctrl+C`, `Alt+D`, `Esc`, `Escape`, `Enter`. Case-insensitive.
std::optional<Chord> parse_chord(std::string_view text);

/// One action's primary chord plus any alternatives; an all-empty binding is disabled.
struct KeyBinding : Chord {
  KeyBinding() = default;
  KeyBinding(Chord primary) : Chord(primary) {}
  std::vector<Chord> extra;
};

/// Whether a terminal event carries this chord.
bool matches_chord(const Chord& chord, const ftxui::Event& event);
/// Whether a terminal event carries the primary or any alternative chord.
bool matches_chord(const KeyBinding& binding, const ftxui::Event& event);

/// Human-readable chord label (`Ctrl+C`, `Alt+Up`, ...).
std::string chord_label(const Chord& chord);
/// All labels joined with ` / `, or `unbound` for a disabled action.
std::string chord_label(const KeyBinding& binding);

struct KeyBindings {
  KeyBinding cancel{Chord{Chord::Kind::Ctrl, 'C'}};
  KeyBinding quit{Chord{Chord::Kind::Ctrl, 'D'}};
  KeyBinding editor{Chord{Chord::Kind::Ctrl, 'G'}};
  KeyBinding history{Chord{Chord::Kind::Ctrl, 'R'}};
  KeyBinding tools{Chord{Chord::Kind::Ctrl, 'O'}};
  KeyBinding thinking{Chord{Chord::Kind::Ctrl, 'T'}};
  KeyBinding hide_tools{Chord{Chord::Kind::Alt, 'O'}};
  KeyBinding paste_image{Chord{Chord::Kind::Ctrl, 'V'}};
  KeyBinding reasoning{Chord{Chord::Kind::BackTab}};
  KeyBinding page_up{Chord{Chord::Kind::Special, 0, Chord::Special::PageUp}};
  KeyBinding page_down{Chord{Chord::Kind::Special, 0, Chord::Special::PageDown}};
  KeyBinding dequeue{Chord{Chord::Kind::Alt, 0, Chord::Special::Up}};
  KeyBinding help{Chord{Chord::Kind::Alt, 'H'}};
  KeyBinding model{Chord{Chord::Kind::Alt, 'M'}};
  KeyBinding sessions{Chord{Chord::Kind::Alt, 'S'}};
  KeyBinding copy{Chord{Chord::Kind::Alt, 'C'}};
  KeyBinding links{Chord{Chord::Kind::Alt, 'L'}};
  KeyBinding model_next{Chord{Chord::Kind::Ctrl, 'P'}};
  KeyBinding retry{Chord{Chord::Kind::Alt, 'R'}};
};

/// Reads `action: chord` lines. A missing file keeps the defaults; one bad line is an error.
Result<KeyBindings> load_keybindings(const std::filesystem::path& path);

}  // namespace booth
