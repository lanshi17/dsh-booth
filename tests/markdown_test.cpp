#include <catch2/catch_test_macros.hpp>

#include <sstream>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>
#include <ftxui/screen/terminal.hpp>

#include "interaction/view.hpp"

using namespace booth;

namespace {

std::string strip_ansi(const std::string& input) {
  std::string out;
  for (std::size_t index = 0; index < input.size();) {
    if (input[index] == '\x1b' && index + 1 < input.size() && input[index + 1] == '[') {
      index += 2;
      while (index < input.size() && !(input[index] >= '@' && input[index] <= '~')) ++index;
      if (index < input.size()) ++index;
      continue;
    }
    out += input[index++];
  }
  return out;
}

std::vector<std::string> split_lines(const std::string& text) {
  std::vector<std::string> lines;
  std::istringstream input(text);
  std::string line;
  while (std::getline(input, line)) {
    while (!line.empty() && (line.back() == ' ' || line.back() == '\r')) line.pop_back();
    if (line.find_first_not_of(' ') == std::string::npos) line.clear();
    lines.push_back(line);
  }
  return lines;
}

std::string render_markdown(const std::string& source, int width, int height) {
  ftxui::Terminal::SetFallbackSize({width, height});
  auto view = markdown_view(source, false);
  ftxui::Screen screen(width, height);
  ftxui::Render(screen, std::move(view));
  const std::string text = strip_ansi(screen.ToString());
  ftxui::Terminal::SetFallbackSize({80, 24});
  return text;
}

}  // namespace

TEST_CASE("markdown tables draw the full omp box") {
  const std::string source =
      "| name | value |\n"
      "|:---|---:|\n"
      "| alpha | 1 |\n"
      "| beta | 2 |\n";
  const std::string text = render_markdown(source, 60, 12);

  REQUIRE(text.find("┌") != std::string::npos);
  REQUIRE(text.find("┬") != std::string::npos);
  REQUIRE(text.find("┐") != std::string::npos);
  REQUIRE(text.find("├") != std::string::npos);
  REQUIRE(text.find("┼") != std::string::npos);
  REQUIRE(text.find("┤") != std::string::npos);
  REQUIRE(text.find("└") != std::string::npos);
  REQUIRE(text.find("┴") != std::string::npos);
  REQUIRE(text.find("┘") != std::string::npos);
  REQUIRE(text.find("│ alpha │     1 │") != std::string::npos);
  REQUIRE(text.find("│ beta  │     2 │") != std::string::npos);
}

TEST_CASE("markdown tables wrap long cells inside the terminal") {
  const std::string long_text =
      "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789abcdefghij";
  const std::string source = "| key | detail |\n|---|---|\n| a | " + long_text + " |\n";
  const std::string text = render_markdown(source, 60, 20);

  REQUIRE(text.find("abcdefghijklmnopqrstuvwxyz") != std::string::npos);
  REQUIRE(text.find("0123456789") != std::string::npos);
  REQUIRE(text.find("└") != std::string::npos);
}

TEST_CASE("markdown code fences indent and space like omp") {
  const std::string source =
      "before\n"
      "```cpp\n"
      "int add(int a, int b) { return a + b; }\n"
      "```\n"
      "after\n";
  const auto lines = split_lines(render_markdown(source, 60, 10));
  std::size_t open = lines.size();
  for (std::size_t index = 0; index < lines.size(); ++index) {
    if (lines[index].find("```cpp") != std::string::npos) open = index;
  }
  REQUIRE(open < lines.size());
  REQUIRE(open + 1 < lines.size());
  REQUIRE(lines[open + 1].rfind("  int add(int a, int b) { return a + b; }", 0) == 0);
  std::size_t close = open + 1;
  while (close < lines.size() && lines[close].find("```") == std::string::npos) ++close;
  REQUIRE(close < lines.size());
  REQUIRE(close + 1 < lines.size());
  REQUIRE(lines[close + 1].empty());
  REQUIRE(close + 2 < lines.size());
  REQUIRE(lines[close + 2].find("after") != std::string::npos);
}

TEST_CASE("thought entries show an omp duration header") {
  TranscriptEntry entry;
  entry.kind = EntryKind::Thought;
  entry.text = "checking the options";
  entry.started = std::chrono::system_clock::now();
  entry.finished = entry.started + std::chrono::seconds(12);
  auto view = entry_element(entry);
  ftxui::Screen screen(40, 6);
  ftxui::Render(screen, std::move(view));
  const std::string text = strip_ansi(screen.ToString());
  REQUIRE(text.find("Thought for 12s") != std::string::npos);
  REQUIRE(text.find("checking the options") != std::string::npos);
}

TEST_CASE("markdown renders inline latex outside code") {
  const std::string math = render_markdown("Energy: $E = mc^2$.\n", 60, 10);
  REQUIRE(math.find("E = mc²") != std::string::npos);
  REQUIRE(math.find("$E") == std::string::npos);

  const std::string currency = render_markdown("costs $5 and $10\n", 60, 10);
  REQUIRE(currency.find("$5") != std::string::npos);
  REQUIRE(currency.find("$10") != std::string::npos);

  const std::string code = render_markdown("`$x^2$`\n```\n$x^2$\n```\n", 60, 12);
  REQUIRE(code.find("$x^2$") != std::string::npos);
}

TEST_CASE("markdown renders display latex blocks") {
  const std::string display = render_markdown("$$\nE = mc^2\n$$\n", 60, 10);
  REQUIRE(display.find("E = mc²") != std::string::npos);
  REQUIRE(display.find("$$") == std::string::npos);

  const std::string aligned =
      render_markdown("\\begin{aligned}\na &= b \\\\ c &= d\n\\end{aligned}\n", 60, 12);
  REQUIRE(aligned.find("a = b") != std::string::npos);
  REQUIRE(aligned.find("c = d") != std::string::npos);
}
