#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/notify.hpp"

using namespace booth;

TEST_CASE("notification preferences default on and parse only valid values") {
  REQUIRE(parse_notifications_enabled("on\n"));
  REQUIRE_FALSE(parse_notifications_enabled("off\n"));
  REQUIRE(parse_notifications_enabled(""));
  REQUIRE(parse_notifications_enabled("enabled\n"));
  REQUIRE(parse_notifications_enabled("off\nextra\n"));
}

TEST_CASE("notification preference persists in state directory") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-notify-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  REQUIRE(notifications_enabled(dir));
  REQUIRE(persist_notifications_enabled(dir, false));
  REQUIRE_FALSE(notifications_enabled(dir));
  REQUIRE(persist_notifications_enabled(dir, true));
  REQUIRE(notifications_enabled(dir));
  {
    std::ofstream out(dir / "notify");
    out << "broken\n";
  }
  REQUIRE(notifications_enabled(dir));
  std::filesystem::remove_all(dir);
}

TEST_CASE("notifications use OSC 9 and a trailing bell") {
  REQUIRE(notification_sequence("booth", "permission needed: bash", false) ==
          "\x1b]9;booth: permission needed: bash\x07\x07");
}

TEST_CASE("notification OSC is wrapped for tmux passthrough") {
  REQUIRE(notification_sequence("booth", "turn complete · 5s", true) ==
          "\x1bPtmux;\x1b\x1b]9;booth: turn complete · 5s\x07\x1b\\\x07");
}
