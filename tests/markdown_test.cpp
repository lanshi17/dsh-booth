#include <catch2/catch_test_macros.hpp>

#include <string>

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
