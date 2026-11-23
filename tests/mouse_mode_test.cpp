#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/mouse_mode.hpp"

using namespace booth;

TEST_CASE("mouse capture defaults off inside tmux") {
  const char* old = ::getenv("TMUX");
  const std::string saved = old == nullptr ? "" : old;
  ::unsetenv("TMUX");
  REQUIRE(default_mouse_enabled());
  ::setenv("TMUX", "/tmp/tmux-1/default,1,0", 1);
  REQUIRE_FALSE(default_mouse_enabled());
  if (old == nullptr) ::unsetenv("TMUX"); else ::setenv("TMUX", saved.c_str(), 1);
}

TEST_CASE("mouse preference persists and falls back on malformed files") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-mouse-mode-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  ::setenv("TMUX", "/tmp/tmux-1/default,1,0", 1);
  REQUIRE_FALSE(load_mouse_enabled(dir));
  REQUIRE(save_mouse_enabled(dir, true));
  REQUIRE(load_mouse_enabled(dir));
  {
    std::ofstream out(dir / "mouse");
    out << "maybe\n";
  }
  REQUIRE_FALSE(load_mouse_enabled(dir));
  std::filesystem::remove_all(dir);
  ::unsetenv("TMUX");
}

TEST_CASE("mouse mode sequences contain the expected DEC modes") {
  REQUIRE(mouse_enable_sequence().find("\x1b[?1000h") != std::string::npos);
  REQUIRE(mouse_enable_sequence().find("\x1b[?1006h") != std::string::npos);
  REQUIRE(mouse_disable_sequence().find("\x1b[?1003l") != std::string::npos);
  REQUIRE(mouse_disable_sequence().find("\x1b[?1015l") != std::string::npos);
}
