#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <sstream>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);
const ftxui::Color kLine = ftxui::Color::RGB(72, 72, 72);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);

ftxui::Element inline_words(const std::string& text, bool dim) {
  ftxui::Elements parts;
  bool code = false;
  bool bold = false;
  std::string segment;
  const auto flush = [&] {
    std::istringstream words(segment);
    std::string word;
    while (words >> word) {
      ftxui::Element part = ftxui::text(word + " ");
      if (code) {
        part = part | ftxui::color(kGreen);
      } else if (dim) {
        part = part | ftxui::dim;
      }
      if (bold) part = part | ftxui::bold;
      parts.push_back(std::move(part));
    }
    segment.clear();
  };
  for (std::size_t index = 0; index < text.size();) {
    if (!code && text[index] == '[') {
      const auto label_end = text.find("](", index);
      const auto url_end = label_end == std::string::npos
                               ? std::string::npos
                               : text.find(')', label_end + 2);
      if (label_end != std::string::npos && url_end != std::string::npos) {
        flush();
        const std::string label = text.substr(index + 1, label_end - index - 1);
        const std::string url = text.substr(label_end + 2, url_end - label_end - 2);
        parts.push_back(ftxui::hyperlink(
            url, ftxui::text(label) | ftxui::color(kAccent) | ftxui::underlined));
        index = url_end + 1;
        continue;
      }
    }
    if (text.compare(index, 2, "**") == 0) {
      flush();
      bold = !bold;
      index += 2;
      continue;
    }
    if (text[index] == '`') {
      flush();
      code = !code;
      ++index;
      continue;
    }
    segment += text[index++];
  }
  flush();
  return ftxui::hflow(std::move(parts));
}

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

std::string trim_copy(std::string value) {
  const auto first = value.find_first_not_of(" \t");
  if (first == std::string::npos) return {};
  const auto last = value.find_last_not_of(" \t");
  return value.substr(first, last - first + 1);
}

std::vector<std::string> table_cells(const std::string& line) {
  std::string body = trim_copy(line);
  if (!body.empty() && body.front() == '|') body.erase(body.begin());
  if (!body.empty() && body.back() == '|') body.pop_back();
  std::vector<std::string> cells;
  std::size_t start = 0;
  while (true) {
    const auto pipe = body.find('|', start);
    cells.push_back(trim_copy(body.substr(start, pipe == std::string::npos ? std::string::npos
                                                                          : pipe - start)));
    if (pipe == std::string::npos) break;
    start = pipe + 1;
  }
  return cells;
}

bool is_table_separator(const std::string& line) {
  bool dash = false;
  for (const char ch : line) {
    if (ch == '-') {
      dash = true;
    } else if (ch != '|' && ch != ':' && ch != ' ' && ch != '\t') {
      return false;
    }
  }
  return dash;
}

bool is_table_start(const std::vector<std::string>& lines, std::size_t index) {
  return index + 1 < lines.size() && lines[index].find('|') != std::string::npos &&
         is_table_separator(lines[index + 1]);
}

std::string truncate_cells(const std::string& text, std::size_t width) {
  std::string out;
  std::size_t used = 0;
  for (std::size_t index = 0; index < text.size();) {
    std::size_t end = index + 1;
    while (end < text.size() && (static_cast<unsigned char>(text[end]) & 0xC0) == 0x80) ++end;
    const std::string glyph = text.substr(index, end - index);
    const int cells = ftxui::string_width(glyph);
    if (used + static_cast<std::size_t>(cells) > width) break;
    out += glyph;
    used += static_cast<std::size_t>(cells);
    index = end;
  }
  return out;
}

ftxui::Element table_view(const std::vector<std::vector<std::string>>& rows) {
  const ftxui::Color border = ftxui::Color::RGB(72, 72, 72);
  std::size_t columns = 0;
  for (const auto& row : rows) columns = std::max(columns, row.size());
  std::vector<std::size_t> widths(columns, 0);
  for (const auto& row : rows) {
    for (std::size_t column = 0; column < row.size(); ++column) {
      widths[column] = std::max(widths[column],
                                static_cast<std::size_t>(ftxui::string_width(row[column])));
    }
  }
  for (auto& width : widths) width = std::min<std::size_t>(width, 24);

  ftxui::Elements body;
  const auto paint = [&](const std::vector<std::string>& row, bool header) {
    ftxui::Elements line;
    line.push_back(ftxui::text("│") | ftxui::color(border));
    for (std::size_t column = 0; column < columns; ++column) {
      std::string cell = column < row.size() ? row[column] : "";
      if (ftxui::string_width(cell) > static_cast<int>(widths[column])) {
        cell = truncate_cells(cell, widths[column]);
      }
      cell += std::string(widths[column] - ftxui::string_width(cell), ' ');
      ftxui::Element part = ftxui::text(" " + cell + " ") | ftxui::color(kText);
      if (header) part = part | ftxui::bold;
      line.push_back(std::move(part));
      line.push_back(ftxui::text("│") | ftxui::color(border));
    }
    body.push_back(ftxui::hbox(std::move(line)));
  };

  paint(rows.front(), true);
  ftxui::Elements separator;
  separator.push_back(ftxui::text("├") | ftxui::color(border));
  for (std::size_t column = 0; column < columns; ++column) {
    std::string rule;
    for (std::size_t cell = 0; cell < widths[column] + 2; ++cell) rule += "─";
    separator.push_back(ftxui::text(rule) | ftxui::color(border));
    separator.push_back(ftxui::text(column + 1 == columns ? "┤" : "┼") | ftxui::color(border));
  }
  body.push_back(ftxui::hbox(std::move(separator)));
  for (std::size_t row = 1; row < rows.size(); ++row) paint(rows[row], false);
  return ftxui::vbox(std::move(body));
}

ftxui::Element render_markdown(const std::string& text, bool dim) {
  ftxui::Elements lines;
  bool code = false;
  std::string lang;
  std::vector<std::string> source;
  {
    std::istringstream input(text);
    std::string line;
    while (std::getline(input, line)) source.push_back(line);
  }
  for (std::size_t index = 0; index < source.size(); ++index) {
    const std::string& line = source[index];
    if (!code && is_table_start(source, index)) {
      std::vector<std::vector<std::string>> rows;
      rows.push_back(table_cells(line));
      std::size_t next = index + 2;
      while (next < source.size() && source[next].find('|') != std::string::npos) {
        rows.push_back(table_cells(source[next]));
        ++next;
      }
      lines.push_back(table_view(rows));
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
      continue;
    }
    if (code) {
      lines.push_back(code_line(line, lang));
      continue;
    }
    if (line.empty()) {
      lines.push_back(ftxui::text(""));
      continue;
    }
    if (line == "---" || line == "***" || line == "___") {
      std::string rule;
      for (std::size_t cell = 0; cell < 40; ++cell) rule += "─";
      lines.push_back(ftxui::text(rule) | ftxui::color(kLine));
      continue;
    }
    if (line.rfind("> ", 0) == 0) {
      lines.push_back(ftxui::hbox({ftxui::text(" │ ") | ftxui::color(kLine),
                                   inline_words(line.substr(2), true) | ftxui::flex}));
      continue;
    }
    std::size_t hashes = 0;
    while (hashes < line.size() && line[hashes] == '#') ++hashes;
    if (hashes > 0 && hashes < line.size() && line[hashes] == ' ') {
      lines.push_back(ftxui::text(" " + line.substr(hashes + 1)) | ftxui::bold |
                      ftxui::color(kAccent));
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
            inline_words(line.substr(dot + 2), dim) | ftxui::flex,
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
          inline_words(line.substr(leading + 2), dim) | ftxui::flex,
      }));
      continue;
    }
    lines.push_back(ftxui::hbox({ftxui::text(" "),
                                 inline_words(line, dim) | ftxui::flex}));
  }
  return ftxui::vbox(std::move(lines));
}


}  // namespace

ftxui::Element markdown_view(const std::string& text, bool dim) {
  return render_markdown(text, dim);
}

ftxui::Element inline_words_view(const std::string& text, bool dim) {
  return inline_words(text, dim);
}

}  // namespace booth
