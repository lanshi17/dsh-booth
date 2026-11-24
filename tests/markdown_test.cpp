#include <catch2/catch_test_macros.hpp>

#include <sstream>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>
#include <ftxui/screen/terminal.hpp>

#include "interaction/view.hpp"
#include "interaction/markdown_table.hpp"

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
    if (input[index] == '\x1b' && index + 1 < input.size() && input[index + 1] == ']') {
      index += 2;
      while (index < input.size()) {
        if (input[index] == '\x07') {
          ++index;
          break;
        }
        if (input[index] == '\x1b' && index + 1 < input.size() && input[index + 1] == '\\') {
          index += 2;
          break;
        }
        ++index;
      }
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

TEST_CASE("markdown renders emphasis and strikethrough") {
  const std::string text = render_markdown("This is *italic* and **bold** and ~~gone~~.\n", 60, 10);
  REQUIRE(text.find("*italic*") == std::string::npos);
  REQUIRE(text.find("italic") != std::string::npos);
  REQUIRE(text.find("**bold**") == std::string::npos);
  REQUIRE(text.find("bold") != std::string::npos);
  REQUIRE(text.find("~~gone~~") == std::string::npos);
  REQUIRE(text.find("gone") != std::string::npos);

  const std::string underscore = render_markdown("snake_case stays literal\n", 60, 10);
  REQUIRE(underscore.find("snake_case") != std::string::npos);
}

TEST_CASE("markdown applies italic and strikethrough styles") {
  auto view = markdown_view("This is *italic* and ~~gone~~.\n", false);
  ftxui::Screen screen(60, 10);
  ftxui::Render(screen, std::move(view));
  bool italic = false;
  bool strike = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.italic) italic = true;
      if (cell.strikethrough) strike = true;
    }
  }
  REQUIRE(italic);
  REQUIRE(strike);
}

TEST_CASE("markdown autolinks bare urls") {
  auto view = markdown_view("Visit https://example.com/docs. and www.test.dev\n", false);
  ftxui::Screen screen(80, 10);
  ftxui::Render(screen, std::move(view));
  bool linked = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      if (screen.CellAt(x, y).hyperlink != 0) linked = true;
    }
  }
  REQUIRE(linked);
  const std::string text = screen.ToString();
  REQUIRE(text.find("https://example.com/docs") != std::string::npos);
  REQUIRE(text.find("www.test.dev") != std::string::npos);
}

TEST_CASE("markdown tables clean inline markup and math") {
  const std::string source =
      "| name | value |\n"
      "|---|---|\n"
      "| **bold** | `code` |\n"
      "| [link](https://x.test) | $x^2$ |\n"
      "| ![alt](img.png) | snake_case |\n";
  const std::string text = render_markdown(source, 60, 14);
  REQUIRE(text.find("**bold**") == std::string::npos);
  REQUIRE(text.find("bold") != std::string::npos);
  REQUIRE(text.find("`code`") == std::string::npos);
  REQUIRE(text.find("code") != std::string::npos);
  REQUIRE(text.find("[link]") == std::string::npos);
  REQUIRE(text.find("link") != std::string::npos);
  REQUIRE(text.find("x²") != std::string::npos);
  REQUIRE(text.find("![alt]") == std::string::npos);
  REQUIRE(text.find("alt") != std::string::npos);
  REQUIRE(text.find("snake_case") != std::string::npos);
}

TEST_CASE("markdown tables style bold, code and link cells") {
  const std::string source =
      "| h | v |\n"
      "|---|---|\n"
      "| **bold** | `code` |\n"
      "| [docs](https://x.test) | 1 |\n";
  auto view = markdown_view(source, false);
  ftxui::Screen screen(60, 12);
  ftxui::Render(screen, std::move(view));
  bool bold = false;
  bool code = false;
  bool linked = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "b" && cell.bold) bold = true;
      if (cell.character == "c" &&
          cell.foreground_color == ftxui::Color::RGB(0, 255, 135)) {
        code = true;
      }
      if (cell.character == "d" && cell.hyperlink != 0) linked = true;
    }
  }
  REQUIRE(bold);
  REQUIRE(code);
  REQUIRE(linked);
}

TEST_CASE("markdown headings distinguish levels") {
  ftxui::Terminal::SetFallbackSize({60, 20});
  auto view = markdown_view("# One\n## Two\n### Three\n#### Four\n", false);
  ftxui::Screen screen(60, 12);
  ftxui::Render(screen, std::move(view));
  bool h1_underlined = false;
  bool h2_accent = false;
  bool h3_text = false;
  bool h4_dim = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "O" && cell.underlined) h1_underlined = true;
      if (cell.character == "w" &&
          cell.foreground_color == ftxui::Color::RGB(0, 175, 255)) {
        h2_accent = true;
      }
      if (cell.character == "h" &&
          cell.foreground_color == ftxui::Color::RGB(228, 228, 228)) {
        h3_text = true;
      }
      if (cell.character == "F" &&
          cell.foreground_color == ftxui::Color::RGB(118, 118, 118)) {
        h4_dim = true;
      }
    }
  }
  REQUIRE(h1_underlined);
  REQUIRE(h2_accent);
  REQUIRE(h3_text);
  REQUIRE(h4_dim);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("markdown rules span the available width") {
  ftxui::Terminal::SetFallbackSize({60, 20});
  const std::string text = render_markdown("---\n", 60, 6);
  std::size_t count = 0;
  for (std::size_t index = 0; index + 3 <= text.size();) {
    if (text.compare(index, 3, "─") == 0) {
      ++count;
      index += 3;
    } else {
      ++index;
    }
  }
  REQUIRE(count >= 50);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("markdown decodes html entities") {
  const std::string text =
      render_markdown("A &amp; B &lt; C &gt; D &quot;E&quot;\n", 60, 10);
  REQUIRE(text.find("A & B < C > D \"E\"") != std::string::npos);

  const std::string table = render_markdown("| x |\n|---|\n| a &amp; b |\n", 60, 10);
  REQUIRE(table.find("a & b") != std::string::npos);
}

TEST_CASE("markdown keeps emphasis on link labels") {
  auto view = markdown_view("**[docs](https://x.test)**\n", false);
  ftxui::Screen screen(60, 8);
  ftxui::Render(screen, std::move(view));
  bool emphasized = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "d" && cell.bold && cell.hyperlink != 0) emphasized = true;
    }
  }
  REQUIRE(emphasized);
}

TEST_CASE("markdown renders nested blockquotes") {
  const std::string text = render_markdown("> outer\n> > inner\n> \n", 60, 10);
  REQUIRE(text.find("outer") != std::string::npos);
  REQUIRE(text.find("inner") != std::string::npos);
  std::size_t bars = 0;
  for (std::size_t index = 0; index + 3 <= text.size();) {
    if (text.compare(index, 3, "│") == 0) {
      ++bars;
      index += 3;
    } else {
      ++index;
    }
  }
  REQUIRE(bars >= 3);
}

TEST_CASE("markdown charts numeric tables") {
  const std::string source =
      "| name | value |\n"
      "|---|---|\n"
      "| a | 1 |\n"
      "| b | 2 |\n"
      "| c | 4 |\n";
  const std::string text = render_markdown(source, 60, 14);
  REQUIRE(text.find("█") != std::string::npos);
  REQUIRE(text.find("░") != std::string::npos);
}

TEST_CASE("markdown leaves non-numeric tables uncharted") {
  const std::string source =
      "| name | value |\n"
      "|---|---|\n"
      "| a | one |\n"
      "| b | two |\n"
      "| c | three |\n";
  const std::string text = render_markdown(source, 60, 14);
  REQUIRE(text.find("█") == std::string::npos);
}

TEST_CASE("markdown setext headings, closing hashes and plus bullets") {
  auto view = markdown_view("Title\n===\nSub\n---\n# Hash ###\n+ plus\n", false);
  ftxui::Screen screen(60, 16);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("Title") != std::string::npos);
  REQUIRE(text.find("Sub") != std::string::npos);
  REQUIRE(text.find("Hash") != std::string::npos);
  REQUIRE(text.find("plus") != std::string::npos);
  REQUIRE(text.find("===") == std::string::npos);
  REQUIRE(text.find("###") == std::string::npos);
  REQUIRE(text.find("+ plus") == std::string::npos);

  bool h1 = false;
  bool h2 = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "T" && cell.underlined) h1 = true;
      if (cell.character == "S" &&
          cell.foreground_color == ftxui::Color::RGB(0, 175, 255)) {
        h2 = true;
      }
    }
  }
  REQUIRE(h1);
  REQUIRE(h2);
}

TEST_CASE("markdown parses link titles and image alt text") {
  auto view = markdown_view("[docs](https://x.test \"Docs\") ![diagram](img.png)\n", false);
  ftxui::Screen screen(60, 8);
  ftxui::Render(screen, std::move(view));
  const std::string text = strip_ansi(screen.ToString());
  REQUIRE(text.find("docs") != std::string::npos);
  REQUIRE(text.find("diagram") != std::string::npos);
  REQUIRE(text.find("Docs") == std::string::npos);
  REQUIRE(text.find("img.png") == std::string::npos);
  REQUIRE(text.find("![]") == std::string::npos);
  bool linked = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      if (screen.CellAt(x, y).hyperlink != 0) linked = true;
    }
  }
  REQUIRE(linked);
}

TEST_CASE("markdown autolinks email addresses") {
  auto view = markdown_view("<dev@example.com>\n", false);
  ftxui::Screen screen(60, 8);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("dev@example.com") != std::string::npos);
  bool linked = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      if (screen.CellAt(x, y).hyperlink != 0) linked = true;
    }
  }
  REQUIRE(linked);
}

TEST_CASE("markdown resolves reference links") {
  auto view = markdown_view("[docs][d] and [d][]\n\n[d]: https://x.test \"T\"\n", false);
  ftxui::Screen screen(70, 10);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("docs") != std::string::npos);
  REQUIRE(text.find("[d]") == std::string::npos);
  REQUIRE(text.find("[d]:") == std::string::npos);
  bool linked = false;
  for (int y = 0; y < screen.dimy(); ++y) {
    for (int x = 0; x < screen.dimx(); ++x) {
      if (screen.CellAt(x, y).hyperlink != 0) linked = true;
    }
  }
  REQUIRE(linked);
}

TEST_CASE("reference links stay raw in code spans") {
  const std::string text = render_markdown("`[d][d]`\n\n[d]: https://x.test\n", 60, 10);
  REQUIRE(text.find("[d][d]") != std::string::npos);
}

TEST_CASE("task list checkboxes render like omp") {
  const std::string text = render_markdown(
      "- [x] shipped\n"
      "- [ ] pending\n"
      "1. [X] numbered\n",
      60, 10);
  REQUIRE(text.find("☑ shipped") != std::string::npos);
  REQUIRE(text.find("☐ pending") != std::string::npos);
  REQUIRE(text.find("1. ☑ numbered") != std::string::npos);
  REQUIRE(text.find("[x]") == std::string::npos);
}

TEST_CASE("markdown images keep their url") {
  ftxui::Terminal::SetFallbackSize({60, 6});
  auto view = markdown_view("see ![diagram](https://example.com/a.png \"title\")", false);
  ftxui::Screen screen(60, 6);
  ftxui::Render(screen, std::move(view));
  bool linked = false;
  for (int y = 0; y < 6; ++y) {
    for (int x = 0; x < 60; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.hyperlink != 0 && screen.Hyperlink(cell.hyperlink) == "https://example.com/a.png") {
        linked = true;
      }
    }
  }
  REQUIRE(linked);
  REQUIRE(strip_ansi(screen.ToString()).find("[image: diagram]") != std::string::npos);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("markdown honors common backslash escapes") {
  const std::string text = render_markdown(
      "\\#tag \\+ plus \\{braces\\} \\\\ slash and \\*literal\\*\n", 60, 10);
  REQUIRE(text.find("#tag") != std::string::npos);
  REQUIRE(text.find("+ plus") != std::string::npos);
  REQUIRE(text.find("{braces}") != std::string::npos);
  REQUIRE(text.find("\\ slash") != std::string::npos);
  REQUIRE(text.find("*literal*") != std::string::npos);
}

TEST_CASE("escaped math delimiters fall back to literal text") {
  const std::string text = render_markdown("a \\( b and c \\[ d\n", 60, 10);
  REQUIRE(text.find("a ( b and c [ d") != std::string::npos);
}

TEST_CASE("diff fences color added and removed lines") {
  ftxui::Terminal::SetFallbackSize({60, 12});
  auto view = markdown_view("```diff\n+added\n-removed\n@@ hunk\n unchanged\n```\n", false);
  ftxui::Screen screen(60, 12);
  ftxui::Render(screen, std::move(view));
  bool added = false;
  bool removed = false;
  bool hunk = false;
  for (int y = 0; y < 12; ++y) {
    for (int x = 0; x < 60; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "+" && cell.foreground_color == ftxui::Color::RGB(0, 255, 135)) added = true;
      if (cell.character == "-" && cell.foreground_color == ftxui::Color::RGB(255, 95, 95)) removed = true;
      if (cell.character == "@" && cell.foreground_color == ftxui::Color::RGB(0, 175, 255)) hunk = true;
    }
  }
  REQUIRE(added);
  REQUIRE(removed);
  REQUIRE(hunk);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("table cells keep escaped and code pipes") {
  const auto cells = table_cells("| `a | b` | c \\| d |");
  REQUIRE(cells.size() == 2);
  REQUIRE(cells[0] == "`a | b`");
  REQUIRE(cells[1] == "c | d");
  const std::string text = render_markdown(
      "| code | value |\n|------|-------|\n| `a | b` | 1 |\n", 60, 10);
  REQUIRE(text.find("a | b") != std::string::npos);
  REQUIRE(text.find("value") != std::string::npos);
}

TEST_CASE("json fences color keys values and literals") {
  ftxui::Terminal::SetFallbackSize({60, 12});
  auto view = markdown_view("```json\n{\"key\": \"value\", \"n\": 12, \"ok\": true}\n```\n", false);
  ftxui::Screen screen(60, 12);
  ftxui::Render(screen, std::move(view));
  bool key = false;
  bool value = false;
  bool number = false;
  for (int y = 0; y < 12; ++y) {
    for (int x = 0; x < 60; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "k" && cell.foreground_color == ftxui::Color::RGB(0, 175, 255)) key = true;
      if (cell.character == "v" && cell.foreground_color == ftxui::Color::RGB(0, 255, 135)) value = true;
      if (cell.character == "1" && cell.foreground_color == ftxui::Color::RGB(215, 175, 135)) number = true;
    }
  }
  REQUIRE(key);
  REQUIRE(value);
  REQUIRE(number);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("triple emphasis toggles bold and italic together") {
  ftxui::Terminal::SetFallbackSize({40, 8});
  auto view = markdown_view("***both*** and ___also___\n", false);
  ftxui::Screen screen(40, 8);
  ftxui::Render(screen, std::move(view));
  bool both = false;
  bool also = false;
  for (int y = 0; y < 8; ++y) {
    for (int x = 0; x < 40; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "b" && cell.bold && cell.italic) both = true;
      if (cell.character == "a" && cell.bold && cell.italic) also = true;
    }
  }
  REQUIRE(both);
  REQUIRE(also);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("rust fences highlight keywords and ignore lifetimes") {
  ftxui::Terminal::SetFallbackSize({60, 12});
  auto view = markdown_view(
      "```rust\nfn main<'a>() {\n    // note\n    let x = &'a 1;\n}\n```\n", false);
  ftxui::Screen screen(60, 12);
  ftxui::Render(screen, std::move(view));
  bool keyword = false;
  bool comment = false;
  bool lifetime = false;
  for (int y = 0; y < 12; ++y) {
    for (int x = 0; x < 60; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "f" && cell.foreground_color == ftxui::Color::RGB(0, 175, 255)) keyword = true;
      if (cell.character == "n" && cell.dim) comment = true;
      if (cell.character == "'" && cell.foreground_color == ftxui::Color::RGB(228, 228, 228)) lifetime = true;
    }
  }
  REQUIRE(keyword);
  REQUIRE(comment);
  REQUIRE(lifetime);
  ftxui::Terminal::SetFallbackSize({80, 24});
}

TEST_CASE("table cells keep strike and image links") {
  const auto cells = styled_cells({"~~gone~~", "![diagram](https://x.test/a.png)"});
  REQUIRE(cells.size() == 2);
  REQUIRE(cells[0].text == "gone");
  REQUIRE(cells[0].strike);
  REQUIRE(cells[1].text == "diagram");
  REQUIRE(cells[1].href == "https://x.test/a.png");

  ftxui::Terminal::SetFallbackSize({60, 10});
  std::vector<std::vector<TableCell>> rows{{TableCell{"head", "", false, false, false, false}},
                                           cells};
  auto view = table_view(rows, {}, 60);
  ftxui::Screen screen(60, 10);
  ftxui::Render(screen, std::move(view));
  bool struck = false;
  bool linked = false;
  for (int y = 0; y < 10; ++y) {
    for (int x = 0; x < 60; ++x) {
      const auto& cell = screen.CellAt(x, y);
      if (cell.character == "g" && cell.strikethrough) struck = true;
      if (cell.hyperlink != 0 && screen.Hyperlink(cell.hyperlink) == "https://x.test/a.png") {
        linked = true;
      }
    }
  }
  REQUIRE(struck);
  REQUIRE(linked);
  ftxui::Terminal::SetFallbackSize({80, 24});
}
