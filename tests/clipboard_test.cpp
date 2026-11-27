#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/clipboard.hpp"

using namespace booth;

TEST_CASE("copy emits an OSC 52 sequence") {
  REQUIRE(clipboard_sequence("hi") == "\x1b]52;c;aGk=\x07");
}

TEST_CASE("inside tmux the sequence is wrapped for passthrough") {
  ::setenv("TMUX", "/tmp/tmux-1000/default,1,0", 1);
  const std::string sequence = clipboard_sequence("hi");
  ::unsetenv("TMUX");
  REQUIRE(sequence == "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
}

TEST_CASE("screen and tmux terminals wrap OSC 52 for passthrough") {
  const char* old_term = ::getenv("TERM");
  const char* old_tmux = ::getenv("TMUX");
  const char* old_force = ::getenv("BOOTH_CLIPBOARD_TMUX");
  const char* old_ssh = ::getenv("SSH_CONNECTION");
  const std::string term = old_term == nullptr ? "" : old_term;
  const std::string tmux = old_tmux == nullptr ? "" : old_tmux;
  const std::string force = old_force == nullptr ? "" : old_force;
  ::unsetenv("TMUX");
  ::unsetenv("BOOTH_CLIPBOARD_TMUX");
  ::setenv("SSH_CONNECTION", "10.0.0.1 1 10.0.0.2 2", 1);
  ::setenv("TERM", "screen-256color", 1);
  REQUIRE(clipboard_sequence("hi") == "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
  ::setenv("TERM", "xterm-256color", 1);
  ::setenv("BOOTH_CLIPBOARD_TMUX", "1", 1);
  REQUIRE(clipboard_sequence("hi") == "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
  ::unsetenv("BOOTH_CLIPBOARD_TMUX");
  ::unsetenv("SSH_CONNECTION");
  if (old_term != nullptr) ::setenv("TERM", term.c_str(), 1); else ::unsetenv("TERM");
  if (old_tmux != nullptr) ::setenv("TMUX", tmux.c_str(), 1); else ::unsetenv("TMUX");
  if (old_force != nullptr) ::setenv("BOOTH_CLIPBOARD_TMUX", force.c_str(), 1); else ::unsetenv("BOOTH_CLIPBOARD_TMUX");
  if (old_ssh != nullptr) ::setenv("SSH_CONNECTION", old_ssh, 1); else ::unsetenv("SSH_CONNECTION");
}

TEST_CASE("clipboard images become base64 attachments") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-clip-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto tool = dir / "wl-paste";
  {
    std::ofstream out(tool);
    out << "#!/bin/sh\nprintf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6ZQAAAABJRU5ErkJggg==' | base64 -d\n";
  }
  std::filesystem::permissions(tool, std::filesystem::perms::owner_exec |
                                         std::filesystem::perms::group_exec |
                                         std::filesystem::perms::others_exec,
                               std::filesystem::perm_options::add);
  const std::string old_path = ::getenv("PATH") == nullptr ? "" : ::getenv("PATH");
  ::setenv("PATH", (dir.string() + ":" + old_path).c_str(), 1);
  const auto image = clipboard_image();
  ::setenv("PATH", old_path.c_str(), 1);
  std::filesystem::remove_all(dir);
  REQUIRE(image.has_value());
  REQUIRE(image->mime_type == "image/png");
  REQUIRE(image->data ==
          "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6ZQAAAABJRU5ErkJggg==");
}

TEST_CASE("a copied image path becomes an attachment") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-clip-path-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto image_path = dir / "shot.png";
  {
    std::ofstream out(image_path, std::ios::binary);
    out << "PNG";
  }
  const auto image = clipboard_image_from_path(image_path.string() + "\n");
  REQUIRE(image.has_value());
  REQUIRE(image->mime_type == "image/png");
  REQUIRE(image->data == "UE5H");
  REQUIRE(image->name == "shot.png");
  REQUIRE_FALSE(clipboard_image_from_path((dir / "notes.txt").string()));
  std::filesystem::remove_all(dir);
}

TEST_CASE("text returned for an image target is not attached as an image") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-clip-text-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto tool = dir / "xclip";
  {
    std::ofstream out(tool);
    out << "#!/bin/sh\nprintf 'clipboard text'\n";
  }
  std::filesystem::permissions(tool, std::filesystem::perms::owner_exec |
                                         std::filesystem::perms::group_exec |
                                         std::filesystem::perms::others_exec,
                               std::filesystem::perm_options::add);
  const std::string old_path = ::getenv("PATH") == nullptr ? "" : ::getenv("PATH");
  const char* old_display = ::getenv("DISPLAY");
  const char* old_ssh = ::getenv("SSH_CONNECTION");
  const std::string display = old_display == nullptr ? "" : old_display;
  ::unsetenv("SSH_CONNECTION");
  ::setenv("DISPLAY", ":test", 1);
  ::setenv("PATH", (dir.string() + ":" + old_path).c_str(), 1);
  REQUIRE_FALSE(clipboard_image().has_value());
  const auto text = clipboard_text();
  ::setenv("PATH", old_path.c_str(), 1);
  if (old_display != nullptr) ::setenv("DISPLAY", display.c_str(), 1); else ::unsetenv("DISPLAY");
  if (old_ssh != nullptr) ::setenv("SSH_CONNECTION", old_ssh, 1); else ::unsetenv("SSH_CONNECTION");
  std::filesystem::remove_all(dir);
  REQUIRE(text.has_value());
  REQUIRE(*text == "clipboard text");
}
