#include "interaction/markdown_table.hpp"
#include "interaction/math.hpp"
#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <map>
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
const ftxui::Color kRed = ftxui::Color::RGB(255, 95, 95);

std::string normalize_lang(std::string lang) {
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
  const bool slash_comment = lang == "cpp" || lang == "ts" || lang == "rust";
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
    if (!rust_lifetime && (ch == '"' || ch == '\'' || (lang == "ts" && ch == '`'))) {
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

bool task_marker(const std::string& text, std::string& box, std::string& rest) {
  if (text.size() < 4 || text[0] != '[' || text[2] != ']' || text[3] != ' ') return false;
  const char state = static_cast<char>(std::tolower(static_cast<unsigned char>(text[1])));
  if (state != ' ' && state != 'x') return false;
  box = state == 'x' ? "☑ " : "☐ ";
  rest = text.substr(4);
  return true;
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

ftxui::Element heading_element(std::size_t level, std::string title) {
  while (!title.empty() && title.back() == ' ') title.pop_back();
  while (!title.empty() && title.back() == '#') title.pop_back();
  while (!title.empty() && title.back() == ' ') title.pop_back();
  ftxui::Element heading = ftxui::text(" " + title) | ftxui::bold;
  if (level == 1) {
    heading = heading | ftxui::color(kAccent) | ftxui::underlined;
  } else if (level == 2) {
    heading = heading | ftxui::color(kAccent);
  } else if (level == 3) {
    heading = heading | ftxui::color(kText);
  } else {
    heading = heading | ftxui::color(kDim);
  }
  return heading;
}

int setext_level(const std::string& line) {
  const auto first = line.find_first_not_of(" \t");
  if (first == std::string::npos) return 0;
  const auto last = line.find_last_not_of(" \t");
  const std::string body = line.substr(first, last - first + 1);
  if (body.empty()) return 0;
  if (body.find_first_not_of('=') == std::string::npos) return 1;
  if (body.find_first_not_of('-') == std::string::npos) return 2;
  return 0;
}

bool can_setext(const std::string& line) {
  const auto first = line.find_first_not_of(" \t");
  if (first == std::string::npos) return false;
  const char ch = line[first];
  if (ch == '-' || ch == '*' || ch == '+' || ch == '>' || ch == '#' || ch == '|' || ch == '`') {
    return false;
  }
  if (std::isdigit(static_cast<unsigned char>(ch)) && line.find(". ", first) != std::string::npos) {
    return false;
  }
  return line.find_first_not_of("=- \t") != std::string::npos;
}

std::string normalize_ref_id(std::string value) {
  std::string out;
  for (const char ch : value) {
    if (ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r') {
      if (!out.empty() && out.back() != ' ') out += ' ';
      continue;
    }
    out += static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  }
  while (!out.empty() && out.back() == ' ') out.pop_back();
  return out;
}

std::optional<std::string> reference_definition(const std::string& line, std::string& id) {
  const auto first = line.find_first_not_of(" \t");
  if (first == std::string::npos || first > 3) return std::nullopt;
  if (line[first] != '[') return std::nullopt;
  const auto close = line.find("]:", first);
  if (close == std::string::npos) return std::nullopt;
  id = normalize_ref_id(line.substr(first + 1, close - first - 1));
  if (id.empty()) return std::nullopt;
  const auto start = line.find_first_not_of(" \t", close + 2);
  if (start == std::string::npos) return std::nullopt;
  std::string url = line.substr(start);
  const auto space = url.find_first_of(" \t");
  if (space != std::string::npos) url = url.substr(0, space);
  if (url.size() >= 2 && url.front() == '<' && url.back() == '>') {
    url = url.substr(1, url.size() - 2);
  }
  if (url.empty()) return std::nullopt;
  return url;
}

std::string resolve_reference_links(const std::string& text,
                                    const std::map<std::string, std::string>& refs) {
  std::string out;
  bool code = false;
  for (std::size_t index = 0; index < text.size();) {
    if (text[index] == '`') {
      code = !code;
      out += text[index++];
      continue;
    }
    const std::size_t open = text[index] == '!' && index + 1 < text.size() && text[index + 1] == '['
                                 ? index + 1
                                 : index;
    if (!code && text[open] == '[') {
      const auto close = text.find(']', open + 1);
      if (close != std::string::npos && close + 1 < text.size() && text[close + 1] == '[') {
        const auto id_end = text.find(']', close + 2);
        if (id_end != std::string::npos) {
          std::string id = text.substr(close + 2, id_end - close - 2);
          if (id.empty()) id = text.substr(open + 1, close - open - 1);
          const auto found = refs.find(normalize_ref_id(id));
          if (found != refs.end()) {
            if (index != open) out += '!';
            out += "[" + text.substr(open + 1, close - open - 1) + "](" + found->second + ")";
            index = id_end + 1;
            continue;
          }
        }
      }
    }
    out += text[index++];
  }
  return out;
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
  std::map<std::string, std::string> refs;
  std::vector<bool> definition(source.size(), false);
  bool ref_code = false;
  for (std::size_t index = 0; index < source.size(); ++index) {
    if (source[index].rfind("```", 0) == 0) {
      ref_code = !ref_code;
      continue;
    }
    if (ref_code) continue;
    std::string id;
    if (const auto url = reference_definition(source[index], id)) {
      refs[id] = *url;
      definition[index] = true;
    }
  }
  const auto render_inline = [&](const std::string& value, bool is_dim) {
    return inline_words_view(resolve_reference_links(value, refs), is_dim);
  };
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
    if (definition[index]) continue;
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
      auto header_cells = table_cells(line);
      for (auto& cell : header_cells) cell = resolve_reference_links(cell, refs);
      rows.push_back(styled_cells(header_cells));
      const auto aligns = table_alignments(source[index + 1]);
      std::size_t next = index + 2;
      while (next < source.size() && source[next].find('|') != std::string::npos) {
        auto cells = table_cells(source[next]);
        for (auto& cell : cells) cell = resolve_reference_links(cell, refs);
        rows.push_back(styled_cells(cells));
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
    if (index + 1 < source.size()) {
      const int level = setext_level(source[index + 1]);
      if (level > 0 && can_setext(line)) {
        lines.push_back(heading_element(static_cast<std::size_t>(level), line));
        ++index;
        continue;
      }
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
                                   render_inline(line.substr(quote_start), true) |
                                       ftxui::flex}));
      continue;
    }
    std::size_t hashes = 0;
    while (hashes < line.size() && line[hashes] == '#') ++hashes;
    if (hashes > 0 && hashes < line.size() && line[hashes] == ' ') {
      lines.push_back(heading_element(hashes, line.substr(hashes + 1)));
      continue;
    }
    const std::size_t leading = line.find_first_not_of(" \t");
    const std::size_t indent = leading == std::string::npos ? 0 : leading;
    if (leading != std::string::npos && std::isdigit(static_cast<unsigned char>(line[leading]))) {
      const auto dot = line.find(". ", leading);
      if (dot != std::string::npos) {
        std::string content = line.substr(dot + 2);
        std::string box;
        std::string rest;
        std::string marker = line.substr(leading, dot - leading + 1) + " ";
        if (task_marker(content, box, rest)) {
          content = rest;
          marker += box;
        }
        lines.push_back(ftxui::hbox({
            ftxui::text(std::string(indent, ' ') + " "),
            ftxui::text(marker) | ftxui::color(kAccent),
            render_inline(content, dim) | ftxui::flex,
        }));
        continue;
      }
    }
    if (leading != std::string::npos &&
        (line[leading] == '-' || line[leading] == '*' || line[leading] == '+') && leading + 1 < line.size() &&
        line[leading + 1] == ' ') {
      std::string content = line.substr(leading + 2);
      std::string box;
      std::string rest;
      std::string marker = "- ";
      if (task_marker(content, box, rest)) {
        content = rest;
        marker += box;
      }
      lines.push_back(ftxui::hbox({
          ftxui::text(std::string(indent, ' ') + " "),
          ftxui::text(marker) | ftxui::color(kAccent),
          render_inline(content, dim) | ftxui::flex,
      }));
      continue;
    }
    lines.push_back(ftxui::hbox({ftxui::text(" "),
                                 render_inline(line, dim) | ftxui::flex}));
  }
  if (math_block) flush_math();
  return ftxui::vbox(std::move(lines));
}


}  // namespace

ftxui::Element markdown_view(const std::string& text, bool dim) {
  return render_markdown(text, dim);
}

}  // namespace booth
