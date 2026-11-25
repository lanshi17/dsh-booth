#include "interaction/markdown_code.hpp"

#include <algorithm>
#include <cctype>
#include <string>
#include <string_view>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/string.hpp>
#include <ftxui/screen/terminal.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);
const ftxui::Color kRed = ftxui::Color::RGB(255, 95, 95);

std::string normalize_lang_impl(std::string lang) {
  for (char& ch : lang) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  if (lang == "c++" || lang == "cxx" || lang == "cc" || lang == "c" || lang == "h" ||
      lang == "hpp" || lang == "hxx") {
    return "cpp";
  }
  if (lang == "ts" || lang == "typescript" || lang == "tsx") return "ts";
  if (lang == "js" || lang == "javascript" || lang == "jsx") return "ts";
  if (lang == "py") return "python";
  if (lang == "sh" || lang == "shell" || lang == "zsh") return "bash";
  if (lang == "rs") return "rust";
  if (lang == "golang") return "go";
  if (lang == "jsonc" || lang == "json5") return "json";
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
  static const std::vector<std::string> go = {
      "break", "case", "chan", "const", "continue", "default", "defer", "else",
      "fallthrough", "for", "func", "go", "goto", "if", "import", "interface", "map",
      "package", "range", "return", "select", "struct", "switch", "type", "var", "true",
      "false", "nil"};
  static const std::vector<std::string> rust = {
      "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
      "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
      "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
      "true", "type", "unsafe", "use", "where", "while", "yield"};
  const auto contains = [&](const std::vector<std::string>& words) {
    return std::find(words.begin(), words.end(), word) != words.end();
  };
  if (lang == "python") return contains(python);
  if (lang == "bash") return contains(bash);
  if (lang == "rust") return contains(rust);
  if (lang == "go") return contains(go);
  if (lang == "cpp" || lang == "ts" || lang == "json") return contains(cpp);
  return false;
}

struct CodeSpan {
  std::string text;
  ftxui::Color color;
  bool dim{false};
};

std::vector<CodeSpan> highlight_line(const std::string& line, const std::string& lang) {
  if (lang == "diff") {
    if (line.empty()) return {};
    ftxui::Color color = kText;
    bool dim = true;
    if (line.rfind("+++", 0) == 0 || line.rfind("---", 0) == 0) {
      color = kDim;
    } else if (line[0] == '+') {
      color = kGreen;
      dim = false;
    } else if (line[0] == '-') {
      color = kRed;
      dim = false;
    } else if (line[0] == '@') {
      color = kAccent;
      dim = false;
    }
    return {CodeSpan{line, color, dim}};
  }
  if (lang == "json") {
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
      if (ch == '"') {
        std::size_t end = index + 1;
        while (end < line.size() && line[end] != '"') {
          if (line[end] == '\\' && end + 1 < line.size()) ++end;
          ++end;
        }
        if (end < line.size()) ++end;
        std::size_t next = end;
        while (next < line.size() && (line[next] == ' ' || line[next] == '\t')) ++next;
        flush();
        const bool key = next < line.size() && line[next] == ':';
        spans.push_back(CodeSpan{line.substr(index, end - index), key ? kAccent : kGreen, false});
        index = end;
        continue;
      }
      if (std::isdigit(static_cast<unsigned char>(ch)) ||
          (ch == '-' && index + 1 < line.size() &&
           std::isdigit(static_cast<unsigned char>(line[index + 1])))) {
        std::size_t end = index + 1;
        while (end < line.size() &&
               (std::isdigit(static_cast<unsigned char>(line[end])) || line[end] == '.' ||
                line[end] == 'e' || line[end] == 'E' || line[end] == '+' || line[end] == '-')) {
          ++end;
        }
        flush();
        spans.push_back(CodeSpan{line.substr(index, end - index), ftxui::Color::RGB(215, 175, 135),
                                 false});
        index = end;
        continue;
      }
      if (std::isalpha(static_cast<unsigned char>(ch))) {
        std::size_t end = index + 1;
        while (end < line.size() && std::isalpha(static_cast<unsigned char>(line[end]))) ++end;
        const std::string word = line.substr(index, end - index);
        if (word == "true" || word == "false" || word == "null") {
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
  const bool hash_comment = lang == "python" || lang == "bash";
  const bool slash_comment = lang == "cpp" || lang == "ts" || lang == "rust" || lang == "go";
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
    const bool rust_lifetime = lang == "rust" && ch == '\'' &&
                               !(index + 2 < line.size() && line[index + 2] == '\'');
    if (!rust_lifetime && (ch == '"' || ch == '\'' || ((lang == "ts" || lang == "go") && ch == '`'))) {
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

std::string expand_code_tabs_impl(const std::string& line) {
  std::string expanded;
  for (const char ch : line) {
    if (ch == '\t') {
      expanded.append(4, ' ');
    } else {
      expanded += ch;
    }
  }
  return expanded;
}

// One FTXUI hbox per visual row. Without this, an overlong hbox makes FTXUI
// shrink every span proportionally, which silently drops characters from the
// middle of the line instead of wrapping it.
std::vector<ftxui::Elements> code_rows_impl(const std::string& line, const std::string& lang,
                                            int width) {
  width = std::max(1, width);
  std::vector<ftxui::Elements> rows(1);
  int used = 0;
  for (const auto& span : highlight_line(line, lang)) {
    std::size_t offset = 0;
    while (offset < span.text.size()) {
      const std::string_view rest(span.text.data() + offset, span.text.size() - offset);
      std::string take;
      int taken = 0;
      for (const auto& glyph : ftxui::Utf8ToGlyphs(rest)) {
        const int cells = ftxui::string_width(glyph);
        if (!take.empty() && used + taken + cells > width) break;
        take += glyph;
        taken += cells;
        if (used + taken >= width) break;
      }
      if (take.empty()) {
        for (const auto& glyph : ftxui::Utf8ToGlyphs(rest)) {
          take = glyph;
          break;
        }
        if (take.empty()) break;
        taken = ftxui::string_width(take);
      }
      ftxui::Element element = ftxui::text(take) | ftxui::color(span.color);
      if (span.dim) element = element | ftxui::dim;
      rows.back().push_back(std::move(element));
      used += taken;
      offset += take.size();
      if (offset < span.text.size()) {
        rows.emplace_back();
        used = 0;
      }
    }
  }
  return rows;
}

}  // namespace

std::string normalize_lang(std::string lang) { return normalize_lang_impl(std::move(lang)); }

std::string expand_code_tabs(const std::string& line) {
  return expand_code_tabs_impl(line);
}

std::vector<ftxui::Elements> code_rows(const std::string& line, const std::string& lang) {
  const int columns = ftxui::Terminal::Size().dimx;
  return code_rows_impl(line, lang, std::max(20, columns - 4));
}

}  // namespace booth
