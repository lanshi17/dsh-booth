#include <catch2/catch_test_macros.hpp>

#include <cstdlib>
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
