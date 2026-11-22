#include <catch2/catch_test_macros.hpp>

#include <expected>
#include <string>
#include <variant>

#include "contract/error.hpp"
#include "contract/event.hpp"
#include "contract/intent.hpp"

using namespace booth;

TEST_CASE("an error names its layer, detail and status") {
  Error error{Layer::Protocol, "bad frame", 42};
  const auto text = to_string(error);
  REQUIRE(text.find("protocol") == 0);
  REQUIRE(text.find("bad frame") != std::string::npos);
  REQUIRE(text.find("42") != std::string::npos);
}

TEST_CASE("a result carries the failure instead of throwing") {
  Result<int> result = std::unexpected(Error{Layer::Config, "no state dir", std::nullopt});
  REQUIRE_FALSE(result.has_value());
  REQUIRE(result.error().layer == Layer::Config);
}

TEST_CASE("a snapshot is one event value") {
  Snapshot snapshot;
  snapshot.phase = DshPhase::Ready;
  snapshot.sessions.push_back(SessionRow{SessionId{"s-1"}, "first", "idle", "/tmp", "auto", 2});

  Event event{snapshot};
  REQUIRE(std::holds_alternative<Snapshot>(event.value));
  const auto& held = std::get<Snapshot>(event.value);
  REQUIRE(held.phase == DshPhase::Ready);
  REQUIRE(held.sessions.front().id == SessionId{"s-1"});
}

TEST_CASE("a permission request keeps its one-shot options") {
  PermissionRequest request;
  request.id = RequestId{"r-1"};
  request.session = SessionId{"s-1"};
  request.tool = "bash";
  request.options = {PermissionOption{"allow", "Allow once", "allow_once"}};

  Event event{request};
  const auto& held = std::get<PermissionRequest>(event.value);
  REQUIRE(held.options.front().id == "allow");
}

TEST_CASE("intents are typed values") {
  Intent prompt{Prompt{SessionId{"s-1"}, "fix the build"}};
  REQUIRE(std::holds_alternative<Prompt>(prompt.value));
  REQUIRE(std::get<Prompt>(prompt.value).text == "fix the build");

  Intent quit{Quit{}};
  REQUIRE(std::holds_alternative<Quit>(quit.value));
}

TEST_CASE("ids compare by value and never across types") {
  REQUIRE(SessionId{"a"} == SessionId{"a"});
  REQUIRE(SessionId{"a"} != SessionId{"b"});
}
