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
}

TEST_CASE("the file overrides known actions and ignores unknown ones") {
  const auto path = temp_file("file");
  {
    std::ofstream out(path);
    out << "# a comment\n"
        << "booth.cancel: Alt+X\n"
        << "booth.unknown: Ctrl+Q\n"
        << "booth.quit: Ctrl+Q\n"
        << "booth.editor: ctrl+t\n";
  }
  auto bindings = load_keybindings(path);
  REQUIRE(bindings.has_value());
  REQUIRE(bindings->cancel.kind == Chord::Kind::Alt);
  REQUIRE(bindings->cancel.key == 'X');
  REQUIRE(bindings->quit.key == 'Q');
  REQUIRE(bindings->editor.key == 'T');
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
