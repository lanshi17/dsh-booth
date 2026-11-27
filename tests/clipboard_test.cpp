#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

#include "interaction/clipboard.hpp"

using namespace booth;

TEST_CASE("copy emits an OSC 52 sequence") {
  REQUIRE(clipboard_sequence("hi") == "\x1b]52;c;aGk=\x07");
}

TEST_CASE("tmux keeps raw OSC 52 and offers a separate passthrough envelope") {
  REQUIRE(clipboard_sequence("hi") == "\x1b]52;c;aGk=\x07");
  REQUIRE(clipboard_passthrough_sequence("hi") ==
          "\x1bPtmux;\x1b\x1b]52;c;aGk=\x07\x1b\\");
}

TEST_CASE("tmux buffer is copied and read over SSH") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-tmux-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto log = dir / "buffer.txt";
  const auto tool = dir / "tmux";
  {
    std::ofstream out(tool);
    out << "#!/bin/sh\n"
           "case \"$1\" in\n"
           "  show-buffer|save-buffer) printf 'tmux buffered';;\n"
           "  load-buffer) cat > \"" << log.string() << "\";;\n"
           "  show-options) printf 'off';;\n"
           "esac\n";
  }
  std::filesystem::permissions(tool, std::filesystem::perms::owner_exec |
                                         std::filesystem::perms::group_exec |
                                         std::filesystem::perms::others_exec,
                               std::filesystem::perm_options::add);
  const std::string old_path = ::getenv("PATH") == nullptr ? "" : ::getenv("PATH");
  const char* old_tmux = ::getenv("TMUX");
  const char* old_ssh = ::getenv("SSH_CONNECTION");
  ::setenv("PATH", (dir.string() + ":" + old_path).c_str(), 1);
  ::setenv("TMUX", "/tmp/tmux-1000/default,1,0", 1);
  ::setenv("SSH_CONNECTION", "10.0.0.1 1 10.0.0.2 2", 1);
  const auto text = clipboard_text();
  REQUIRE(text.has_value());
  REQUIRE(*text == "tmux buffered");
  std::ostringstream captured;
  auto* previous = std::cout.rdbuf(captured.rdbuf());
  REQUIRE(copy_to_clipboard("hello"));
  std::cout.rdbuf(previous);
  std::ifstream input(log);
  const std::string copied((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  REQUIRE(copied == "hello");
  REQUIRE(captured.str().find("\x1b]52;c;") != std::string::npos);
  ::setenv("PATH", old_path.c_str(), 1);
  if (old_tmux != nullptr) ::setenv("TMUX", old_tmux, 1); else ::unsetenv("TMUX");
  if (old_ssh != nullptr) ::setenv("SSH_CONNECTION", old_ssh, 1); else ::unsetenv("SSH_CONNECTION");
  std::filesystem::remove_all(dir);
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
