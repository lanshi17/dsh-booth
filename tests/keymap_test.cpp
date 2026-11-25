#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/keymap.hpp"

using namespace booth;

namespace {

std::filesystem::path temp_file(std::string_view name) {
  const auto path = std::filesystem::temp_directory_path() /
                    ("booth-keymap-" + std::string(name) + "-" + std::to_string(::getpid()));
  std::filesystem::remove(path);
  return path;
}

}  // namespace

TEST_CASE("chords render back to labels") {
  REQUIRE(chord_label(Chord{Chord::Kind::Ctrl, 'C'}) == "Ctrl+C");
  REQUIRE(chord_label(Chord{Chord::Kind::Alt, 0, Chord::Special::Up}) == "Alt+Up");
  REQUIRE(chord_label(Chord{Chord::Kind::Enter, 0}) == "Enter");
  REQUIRE(chord_label(Chord{Chord::Kind::Escape, 0}) == "Esc");
  REQUIRE(chord_label(Chord{Chord::Kind::BackTab, 0}) == "Shift+Tab");
}

TEST_CASE("chords parse from the UI spelling") {
  const auto ctrl_c = parse_chord("Ctrl+C");
  REQUIRE(ctrl_c.has_value());
  REQUIRE(ctrl_c->kind == Chord::Kind::Ctrl);
  REQUIRE(ctrl_c->key == 'C');

  const auto alt_d = parse_chord(" alt+d ");
  REQUIRE(alt_d.has_value());
  REQUIRE(alt_d->kind == Chord::Kind::Alt);
  REQUIRE(alt_d->key == 'D');

  REQUIRE(parse_chord("Esc")->kind == Chord::Kind::Escape);
  REQUIRE(parse_chord("enter")->kind == Chord::Kind::Enter);
  REQUIRE_FALSE(parse_chord("Hyper+X").has_value());
  REQUIRE_FALSE(parse_chord("Ctrl+1").has_value());
  REQUIRE_FALSE(parse_chord("C").has_value());
}

TEST_CASE("a missing file keeps the defaults") {
  const auto path = temp_file("missing");
  auto bindings = load_keybindings(path);
  REQUIRE(bindings.has_value());
  REQUIRE(bindings->cancel.kind == Chord::Kind::Ctrl);
  REQUIRE(bindings->cancel.key == 'C');
  REQUIRE(bindings->quit.key == 'D');
  REQUIRE(bindings->editor.kind == Chord::Kind::Ctrl);
  REQUIRE(bindings->editor.key == 'G');
  REQUIRE(bindings->history.key == 'R');
  REQUIRE(bindings->tools.key == 'O');
  REQUIRE(bindings->thinking.key == 'T');
  REQUIRE(bindings->hide_tools.kind == Chord::Kind::Alt);
  REQUIRE(bindings->hide_tools.key == 'O');
  REQUIRE(bindings->paste_image.kind == Chord::Kind::Ctrl);
  REQUIRE(bindings->paste_image.key == 'V');
  REQUIRE(bindings->links.kind == Chord::Kind::Alt);
  REQUIRE(bindings->links.key == 'L');
}

TEST_CASE("the file overrides known actions and ignores unknown ones") {
  const auto path = temp_file("file");
  {
    std::ofstream out(path);
    out << "# a comment\n"
        << "booth.cancel: Alt+X\n"
        << "booth.unknown: Ctrl+Q\n"
        << "booth.quit: Ctrl+Q\n"
        << "booth.editor: ctrl+t\n"
        << "booth.history: alt+r\n"
        << "booth.tools: ctrl+u\n"
        << "booth.thinking: ctrl+w\n"
        << "booth.hide_tools: alt+h\n"
        << "booth.paste_image: ctrl+p\n";
  }
  auto bindings = load_keybindings(path);
  REQUIRE(bindings.has_value());
  REQUIRE(bindings->cancel.kind == Chord::Kind::Alt);
  REQUIRE(bindings->cancel.key == 'X');
  REQUIRE(bindings->quit.key == 'Q');
  REQUIRE(bindings->editor.key == 'T');
  REQUIRE(bindings->history.key == 'R');
  REQUIRE(bindings->tools.key == 'U');
  REQUIRE(bindings->thinking.key == 'W');
  REQUIRE(bindings->hide_tools.key == 'H');
  REQUIRE(bindings->paste_image.key == 'P');
  std::filesystem::remove(path);
}

TEST_CASE("bindings accept alternatives and an empty list disables an action") {
  const auto path = temp_file("arrays");
  {
    std::ofstream out(path);
    out << "booth.help: [Alt+H, Ctrl+H]\n"
        << "booth.cancel: []\n";
  }
  auto bindings = load_keybindings(path);
  REQUIRE(bindings.has_value());
  REQUIRE(bindings->help.kind == Chord::Kind::Alt);
  REQUIRE(bindings->help.key == 'H');
  REQUIRE(bindings->help.extra.size() == 1);
  REQUIRE(bindings->help.extra[0].kind == Chord::Kind::Ctrl);
  REQUIRE(matches_chord(bindings->help, ftxui::Event::Special("\x1bH")));
  REQUIRE(matches_chord(bindings->help, ftxui::Event::Special("\x08")));
  REQUIRE(chord_label(bindings->help) == "Alt+H / Ctrl+H");
  REQUIRE(bindings->cancel.kind == Chord::Kind::None);
  REQUIRE_FALSE(matches_chord(bindings->cancel, ftxui::Event::Special("\x03")));
  REQUIRE(chord_label(bindings->cancel) == "unbound");
  std::filesystem::remove(path);
}

TEST_CASE("a malformed line is a config error") {
  const auto path = temp_file("bad");
  {
    std::ofstream out(path);
    out << "booth.cancel Ctrl+C\n";
  }
  auto bindings = load_keybindings(path);
  REQUIRE_FALSE(bindings.has_value());
  REQUIRE(bindings.error().layer == Layer::Config);
  std::filesystem::remove(path);
}

TEST_CASE("navigation chords parse and match terminal sequences") {
  const auto page = parse_chord("PageUp");
  REQUIRE(page.has_value());
  REQUIRE(page->kind == Chord::Kind::Special);
  REQUIRE(page->special == Chord::Special::PageUp);

  const auto shift_tab = parse_chord("Shift+Tab");
  REQUIRE(shift_tab.has_value());
  REQUIRE(shift_tab->kind == Chord::Kind::BackTab);

  const auto alt_up = parse_chord("Alt+Up");
  REQUIRE(alt_up.has_value());
  REQUIRE(alt_up->kind == Chord::Kind::Alt);
  REQUIRE(alt_up->special == Chord::Special::Up);
  const Chord default_dequeue{Chord::Kind::Alt, 0, Chord::Special::Up};
  REQUIRE(matches_chord(default_dequeue, ftxui::Event::Special("\x1b[1;3A")));
  REQUIRE(matches_chord(*page, ftxui::Event::Special("\x1b[5~")));
}
