#include "interaction/trace_report.hpp"

#include <filesystem>
#include <fstream>
#include <string>

#include <catch2/catch_test_macros.hpp>

TEST_CASE("trace report writes escaped session data") {
  booth::AppState state;
  state.session = "s-trace";
  state.model = "model";
  booth::TranscriptEntry tool;
  tool.kind = booth::EntryKind::Tool;
  tool.tool = "read";
  tool.status = "error";
  tool.text = "a < b & c";
  tool.started = std::chrono::system_clock::now();
  tool.finished = tool.started + std::chrono::milliseconds(12);
  state.entries.push_back(tool);
  const auto path = std::filesystem::temp_directory_path() / "booth-trace-test.html";
  std::string error;
  REQUIRE(booth::write_trace_report(state, path, error));
  std::ifstream input(path);
  const std::string html((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  REQUIRE(html.find("s-trace") != std::string::npos);
  REQUIRE(html.find("a &lt; b &amp; c") != std::string::npos);
  REQUIRE(html.find("12 ms") != std::string::npos);
  std::filesystem::remove(path);
}
