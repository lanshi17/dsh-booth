#include "interaction/markdown_table.hpp"
#include "interaction/math.hpp"
#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <sstream>
#include <string>
#include <string_view>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);
const ftxui::Color kLine = ftxui::Color::RGB(72, 72, 72);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);

std::string normalize_lang(std::string lang) {
  for (char& ch : lang) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  if (lang == "c++" || lang == "cxx") return "cpp";
  if (lang == "ts" || lang == "typescript") return "ts";
  if (lang == "js" || lang == "javascript") return "ts";
  if (lang == "py") return "python";
  if (lang == "sh" || lang == "shell") return "bash";
  return lang;
}

bool is_keyword(const std::string& lang, const std::string& word) {
  static const std::vector<std::string> cpp = {
      "if", "else", "for", "while", "return", "function", "const", "let", "var", "class",
      "struct", "void", "int", "bool", "auto", "import", "export", "await", "async", "new",
      "switch", "case", "break", "continue", "namespace", "using", "template", "public",
      "private", "protected", "static", "true", "false", "null", "nullptr", "undefined"};
  static const std::vector<std::string> python = {
      "def", "class", "import", "from", "return", "if", "elif", "else", "for", "while", "in",
      "not", "and", "or", "True", "False", "None", "with", "as", "lambda", "try", "except",
      "finally", "raise", "yield", "async", "await"};
  static const std::vector<std::string> bash = {
      "if", "then", "else", "fi", "for", "do", "done", "while", "function", "export", "local",
      "echo", "cd", "exit", "set"};
  const auto contains = [&](const std::vector<std::string>& words) {
    return std::find(words.begin(), words.end(), word) != words.end();
  };
  if (lang == "python") return contains(python);
  if (lang == "bash") return contains(bash);
  if (lang == "cpp" || lang == "ts" || lang == "json") return contains(cpp);
  return false;
}

struct CodeSpan {
  std::string text;
  ftxui::Color color;
  bool dim{false};
};

std::vector<CodeSpan> highlight_line(const std::string& line, const std::string& lang) {
  const bool hash_comment = lang == "python" || lang == "bash";
  const bool slash_comment = lang == "cpp" || lang == "ts";
  std::vector<CodeSpan> spans;
  std::string plain;
  const auto flush = [&] {
    if (!plain.empty()) {
      spans.push_back(CodeSpan{plain, kText, false});
      plain.clear();
    }
  };
  for (std::size_t index = 0; index < line.size();) {
    const char ch = line[index];
    if ((hash_comment && ch == '#') || (slash_comment && ch == '/' &&
                                        index + 1 < line.size() && line[index + 1] == '/')) {
      flush();
      spans.push_back(CodeSpan{line.substr(index), kDim, true});
      break;
    }
    if (ch == '"' || ch == '\'' || (lang == "ts" && ch == '`')) {
      const char quote = ch;
      std::size_t end = index + 1;
      while (end < line.size() && line[end] != quote) {
        if (line[end] == '\\' && end + 1 < line.size()) ++end;
        ++end;
      }
      if (end < line.size()) ++end;
      flush();
      spans.push_back(CodeSpan{line.substr(index, end - index), kGreen, false});
      index = end;
      continue;
    }
    if (std::isdigit(static_cast<unsigned char>(ch)) &&
        (index == 0 || !std::isalnum(static_cast<unsigned char>(line[index - 1])))) {
      std::size_t end = index + 1;
      while (end < line.size() &&
             (std::isalnum(static_cast<unsigned char>(line[end])) || line[end] == '.')) {
        ++end;
      }
      flush();
      spans.push_back(CodeSpan{line.substr(index, end - index), ftxui::Color::RGB(215, 175, 135),
                               false});
      index = end;
      continue;
    }
    if (std::isalpha(static_cast<unsigned char>(ch)) || ch == '_') {
      std::size_t end = index + 1;
      while (end < line.size() &&
             (std::isalnum(static_cast<unsigned char>(line[end])) || line[end] == '_')) {
        ++end;
      }
      const std::string word = line.substr(index, end - index);
      if (is_keyword(lang, word)) {
        flush();
        spans.push_back(CodeSpan{word, kAccent, false});
      } else {
        plain += word;
      }
      index = end;
      continue;
    }
    plain += ch;
    ++index;
  }
  flush();
  return spans;
}

ftxui::Element code_line(const std::string& line, const std::string& lang) {
  if (line.empty()) return ftxui::text("");
  ftxui::Elements spans;
  for (const auto& span : highlight_line(line, lang)) {
    ftxui::Element element = ftxui::text(span.text) | ftxui::color(span.color);
    if (span.dim) element = element | ftxui::dim;
    spans.push_back(std::move(element));
  }
  return ftxui::hbox(std::move(spans));
}

bool is_math_environment(std::string_view env) {
  static constexpr std::string_view kEnvironments[] = {
      "matrix", "smallmatrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix",
      "cases", "dcases", "rcases", "drcases", "aligned", "alignedat", "align", "alignat",
      "split", "gathered", "gatheredat", "gather", "multline", "equation", "eqnarray",
      "array", "subarray",
  };
  for (const auto candidate : kEnvironments) {
    if (env == candidate) return true;
    if (env.size() == candidate.size() + 1 && env.back() == '*' && env.substr(0, env.size() - 1) == candidate) {
      return true;
    }
  }
  return false;
}

ftxui::Element render_markdown(const std::string& text, bool dim) {
  ftxui::Elements lines;
  bool code = false;
  bool math_block = false;
  std::string math_close;
  std::string math_source;
  std::string lang;
  std::vector<std::string> source;
  {
    std::istringstream input(text);
    std::string line;
    while (std::getline(input, line)) source.push_back(line);
  }
  const auto append_math = [&](std::string_view part) {
    if (!math_source.empty() && !part.empty()) math_source += '\n';
    math_source += part;
  };
  const auto flush_math = [&] {
    const auto rendered = math_block_lines(math_source);
    if (!rendered.empty()) {
      for (const auto& line : rendered) {
        ftxui::Element row = ftxui::text("  " + line) | ftxui::color(kText);
        if (dim) row = std::move(row) | ftxui::dim;
        lines.push_back(std::move(row));
      }
      lines.push_back(ftxui::text(""));
    }
    math_block = false;
    math_close.clear();
    math_source.clear();
  };
  for (std::size_t index = 0; index < source.size(); ++index) {
    const std::string& line = source[index];
    if (math_block) {
      const auto close = line.find(math_close);
      if (close == std::string::npos) {
        append_math(line);
      } else {
        append_math(line.substr(0, close));
        flush_math();
      }
      continue;
    }
    if (!code) {
      if (line.rfind("$$", 0) == 0 && line.find("$$", 2) == std::string::npos) {
        math_block = true;
        math_close = "$$";
        append_math(line.substr(2));
        continue;
      }
      if (line.rfind("\\begin{", 0) == 0) {
        const auto brace = line.find('}');
        if (brace != std::string::npos) {
          const std::string env = line.substr(7, brace - 7);
          if (is_math_environment(env)) {
            math_block = true;
            math_close = "\\end{" + env + "}";
            append_math(line);
            continue;
          }
        }
      }
    }
    if (!code && is_table_start(source, index)) {
      std::vector<std::vector<TableCell>> rows;
      rows.push_back(styled_cells(table_cells(line)));
      const auto aligns = table_alignments(source[index + 1]);
      std::size_t next = index + 2;
      while (next < source.size() && source[next].find('|') != std::string::npos) {
        rows.push_back(styled_cells(table_cells(source[next])));
        ++next;
      }
      const int max_width = std::max(20, ftxui::Terminal::Size().dimx - 4);
      lines.push_back(table_view(rows, aligns, max_width));
      const auto chart = table_chart_lines(rows, max_width);
      if (!chart.empty()) {
        lines.push_back(ftxui::text(""));
        for (const auto& bar : chart) {
          lines.push_back(ftxui::text("  " + bar) | ftxui::color(kText));
        }
      }
      index = next - 1;
      continue;
    }
    if (line.rfind("```", 0) == 0) {
      code = !code;
      lang = code ? normalize_lang(line.substr(3)) : std::string{};
      lines.push_back(ftxui::hbox({
          ftxui::text(" "),
          ftxui::text("```") | ftxui::color(kLine),
          ftxui::text(line.substr(3)),
      }));
      if (!code && index + 1 < source.size() && !source[index + 1].empty()) {
        lines.push_back(ftxui::text(""));
      }
      continue;
    }
    if (code) {
      lines.push_back(ftxui::hbox({ftxui::text("  "), code_line(line, lang)}));
      continue;
    }
    if (line.empty()) {
      lines.push_back(ftxui::text(""));
      continue;
    }
    if (line == "---" || line == "***" || line == "___") {
      const int width = std::min(80, std::max(20, ftxui::Terminal::Size().dimx - 4));
      std::string rule;
      for (int cell = 0; cell < width; ++cell) rule += "─";
      lines.push_back(ftxui::text(rule) | ftxui::color(kLine));
      continue;
    }
    std::size_t quote_depth = 0;
    std::size_t quote_start = 0;
    while (quote_start < line.size() && line[quote_start] == '>') {
      ++quote_depth;
      ++quote_start;
      if (quote_start < line.size() && line[quote_start] == ' ') ++quote_start;
    }
    if (quote_depth > 0) {
      std::string prefix;
      for (std::size_t depth = 0; depth < quote_depth; ++depth) prefix += "│ ";
      lines.push_back(ftxui::hbox({ftxui::text(prefix) | ftxui::color(kLine),
                                   inline_words_view(line.substr(quote_start), true) |
                                       ftxui::flex}));
      continue;
    }
    std::size_t hashes = 0;
    while (hashes < line.size() && line[hashes] == '#') ++hashes;
    if (hashes > 0 && hashes < line.size() && line[hashes] == ' ') {
      ftxui::Element heading = ftxui::text(" " + line.substr(hashes + 1)) | ftxui::bold;
      if (hashes == 1) {
        heading = heading | ftxui::color(kAccent) | ftxui::underlined;
      } else if (hashes == 2) {
        heading = heading | ftxui::color(kAccent);
      } else if (hashes == 3) {
        heading = heading | ftxui::color(kText);
      } else {
        heading = heading | ftxui::color(kDim);
      }
      lines.push_back(std::move(heading));
      continue;
    }
    const std::size_t leading = line.find_first_not_of(" \t");
    const std::size_t indent = leading == std::string::npos ? 0 : leading;
    if (leading != std::string::npos && std::isdigit(static_cast<unsigned char>(line[leading]))) {
      const auto dot = line.find(". ", leading);
      if (dot != std::string::npos) {
        lines.push_back(ftxui::hbox({
            ftxui::text(std::string(indent, ' ') + " "),
            ftxui::text(line.substr(leading, dot - leading + 1) + " ") |
                ftxui::color(kAccent),
            inline_words_view(line.substr(dot + 2), dim) | ftxui::flex,
        }));
        continue;
      }
    }
    if (leading != std::string::npos &&
        (line[leading] == '-' || line[leading] == '*') && leading + 1 < line.size() &&
        line[leading + 1] == ' ') {
      lines.push_back(ftxui::hbox({
          ftxui::text(std::string(indent, ' ') + " "),
          ftxui::text("- ") | ftxui::color(kAccent),
          inline_words_view(line.substr(leading + 2), dim) | ftxui::flex,
      }));
      continue;
    }
    lines.push_back(ftxui::hbox({ftxui::text(" "),
                                 inline_words_view(line, dim) | ftxui::flex}));
  }
  if (math_block) flush_math();
  return ftxui::vbox(std::move(lines));
}


}  // namespace

ftxui::Element markdown_view(const std::string& text, bool dim) {
  return render_markdown(text, dim);
}

}  // namespace booth
