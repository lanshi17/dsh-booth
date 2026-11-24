#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/link_picker.hpp"

using namespace booth;

namespace {
TranscriptEntry entry(EntryKind kind, std::string text) {
  TranscriptEntry value;
  value.kind = kind;
  value.text = std::move(text);
  return value;
}
}

TEST_CASE("link picker extracts markdown and bare URLs in first-seen order") {
  TranscriptEntry tool = entry(EntryKind::Tool, "");
  tool.output = "see [tool docs](https://tool.test/docs) and https://ignored.example/tool";
  const std::vector<TranscriptEntry> entries{
      entry(EntryKind::User, "read [docs](https://example.com/docs) and https://example.com/docs."),
      tool,
      entry(EntryKind::Assistant, "mail me at mailto:dev@example.com or https://other.test/x?q=1"),
      entry(EntryKind::Assistant, "https://example.com/docs"),
  };
  const auto links = collect_transcript_links(entries);
  REQUIRE(links.size() == 5);
  REQUIRE(links[0].url == "https://example.com/docs");
  REQUIRE(links[0].host == "example.com");
  REQUIRE(links[1].url == "https://tool.test/docs");
  REQUIRE(links[2].url == "https://ignored.example/tool");
  REQUIRE(links[3].url == "mailto:dev@example.com");
  REQUIRE(links[4].url == "https://other.test/x?q=1");
  REQUIRE(links[0].hint == "a");
  REQUIRE(links[4].hint == "e");
}

TEST_CASE("link picker filters non-openable URLs and assigns stable hints") {
  const std::vector<TranscriptEntry> entries{
      entry(EntryKind::Assistant, "javascript:alert(1) file:///tmp/x https://ok.test"),
  };
  const auto links = collect_transcript_links(entries);
  REQUIRE(links.size() == 1);
  REQUIRE(links[0].url == "https://ok.test");
  REQUIRE(link_hint(0) == "a");
  REQUIRE(link_hint(25) == "z");
  REQUIRE(link_hint(26) == "A");
  REQUIRE(link_hint(52) == "?");
}

TEST_CASE("link picker opens, navigates and selects hints without launching a browser") {
  AppState state;
  state.entries.push_back(entry(EntryKind::Assistant, "https://a.test https://b.test"));
  LinkPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.visible());
  REQUIRE(picker.items().size() == 2);
  REQUIRE(picker.index() == 0);
  ftxui::Event down = ftxui::Event::ArrowDown;
  REQUIRE(picker.key(down, state));
  REQUIRE(picker.index() == 1);
  REQUIRE(picker.select_hint('a'));
  REQUIRE(picker.index() == 0);
  ftxui::Event escape = ftxui::Event::Escape;
  REQUIRE(picker.key(escape, state));
  REQUIRE_FALSE(picker.visible());
}

TEST_CASE("link picker renders an empty state") {
  LinkPicker picker;
  AppState state;
  REQUIRE_FALSE(picker.open(state));
  REQUIRE(picker.visible());
  auto view = link_picker_view(picker.items(), picker.index());
  ftxui::Screen screen(70, 8);
  ftxui::Render(screen, view);
  REQUIRE(screen.ToString().find("no links") != std::string::npos);
}
