#include "interaction/math.hpp"

#include <algorithm>
#include <string>
#include <string_view>
#include <vector>

#include <ftxui/screen/string.hpp>

namespace booth {
namespace {

std::string trim(std::string value) {
  const auto first = value.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = value.find_last_not_of(" \t\r\n");
  return value.substr(first, last - first + 1);
}

bool starts_with(std::string_view text, std::string_view prefix) {
  return text.size() >= prefix.size() && text.compare(0, prefix.size(), prefix) == 0;
}

bool ends_with(std::string_view text, std::string_view suffix) {
  return text.size() >= suffix.size() &&
         text.compare(text.size() - suffix.size(), suffix.size(), suffix) == 0;
}

int display_width(const std::string& text) { return ftxui::string_width(text); }

std::string repeat(const std::string& glyph, int count) {
  std::string out;
  for (int index = 0; index < std::max(0, count); ++index) out += glyph;
  return out;
}

std::string centered(const std::string& text, int width) {
  const int pad = std::max(0, width - display_width(text));
  return std::string(static_cast<std::size_t>(pad / 2), ' ') + text +
         std::string(static_cast<std::size_t>(pad - pad / 2), ' ');
}

bool read_group(std::string_view source, std::size_t& index, std::string& out) {
  while (index < source.size() && (source[index] == ' ' || source[index] == '\t')) ++index;
  if (index >= source.size() || source[index] != '{') return false;
  ++index;
  int depth = 1;
  const std::size_t start = index;
  while (index < source.size()) {
    if (source[index] == '{') ++depth;
    else if (source[index] == '}') {
      --depth;
      if (depth == 0) {
        out = std::string(source.substr(start, index - start));
        ++index;
        return true;
      }
    }
    ++index;
  }
  return false;
}

std::vector<std::string> split_top_level(std::string_view text, std::string_view token) {
  std::vector<std::string> parts;
  int depth = 0;
  std::size_t start = 0;
  for (std::size_t index = 0; index < text.size();) {
    if (text[index] == '{') {
      ++depth;
      ++index;
      continue;
    }
    if (text[index] == '}') {
      --depth;
      ++index;
      continue;
    }
    if (depth == 0 && text.compare(index, token.size(), token) == 0) {
      parts.push_back(std::string(text.substr(start, index - start)));
      index += token.size();
      start = index;
      continue;
    }
    ++index;
  }
  parts.push_back(std::string(text.substr(start)));
  return parts;
}

std::vector<std::string> radical_lines(const std::string& body, const std::string& root) {
  const std::string text = latex_to_unicode(body);
  const int width = std::max(3, display_width(text));
  std::string prefix = "√ ";
  if (!root.empty()) prefix = root == "3" ? "∛ " : "√[" + root + "] ";
  const int prefix_width = display_width(prefix);
  std::string top(static_cast<std::size_t>(std::max(0, prefix_width - 1)), ' ');
  top += "┌";
  top += repeat("─", width);
  return {top, prefix + text};
}

std::vector<std::string> fraction_lines(const std::string& numerator, const std::string& denominator) {
  std::vector<std::string> top = math_block_lines(numerator);
  std::vector<std::string> bottom = math_block_lines(denominator);
  if (top.empty()) top.push_back(latex_to_unicode(numerator));
  if (bottom.empty()) bottom.push_back(latex_to_unicode(denominator));
  int width = 3;
  for (const auto& line : top) width = std::max(width, display_width(line));
  for (const auto& line : bottom) width = std::max(width, display_width(line));
  std::vector<std::string> out;
  for (const auto& line : top) out.push_back(centered(line, width));
  out.push_back(repeat("─", width));
  for (const auto& line : bottom) out.push_back(centered(line, width));
  return out;
}

struct Delimiters {
  const char* top_left;
  const char* middle_left;
  const char* bottom_left;
  const char* top_right;
  const char* middle_right;
  const char* bottom_right;
};
bool matrix_delimiters(const std::string& env, Delimiters& out) {
  if (env == "pmatrix") out = {"⎛", "⎜", "⎝", "⎞", "⎟", "⎠"};
  else if (env == "bmatrix") out = {"⎡", "⎢", "⎣", "⎤", "⎥", "⎦"};
  else if (env == "Bmatrix") out = {"⎧", "⎨", "⎩", "⎫", "⎬", "⎭"};
  else if (env == "vmatrix" || env == "Vmatrix") out = {"│", "│", "│", "│", "│", "│"};
  else if (env == "cases" || env == "dcases" || env == "rcases" || env == "drcases")
    out = {"⎧", "⎨", "⎩", env.rfind("rcases", 0) == 0 ? "}" : "", "", ""};
  else if (env == "matrix" || env == "smallmatrix" || env == "array") out = {"", "", "", "", "", ""};
  else return false;
  return true;
}

std::vector<std::string> matrix_lines(const std::string& env, const std::string& body) {
  Delimiters delimiters{};
  if (!matrix_delimiters(env, delimiters)) return {};
  const auto row_texts = split_top_level(body, "\\\\");
  std::vector<std::vector<std::string>> rows;
  for (const auto& row_text : row_texts) {
    if (row_text.empty()) continue;
    std::vector<std::string> cells;
    for (const auto& cell : split_top_level(row_text, "&")) {
      cells.push_back(latex_to_unicode(cell));
    }
    rows.push_back(std::move(cells));
  }
  if (rows.empty()) return {};
  std::size_t columns = 0;
  for (const auto& row : rows) columns = std::max(columns, row.size());
  std::vector<int> widths(columns, 0);
  for (const auto& row : rows) {
    for (std::size_t column = 0; column < row.size(); ++column) {
      widths[column] = std::max(widths[column], display_width(row[column]));
    }
  }
  std::vector<std::string> out;
  for (std::size_t row_index = 0; row_index < rows.size(); ++row_index) {
    const bool top = row_index == 0;
    const bool bottom = row_index + 1 == rows.size();
    const char* left = top ? delimiters.top_left : bottom ? delimiters.bottom_left : delimiters.middle_left;
    const char* right = top ? delimiters.top_right : bottom ? delimiters.bottom_right : delimiters.middle_right;
    std::string line = left;
    if (*left != '\0') line += ' ';
    for (std::size_t column = 0; column < columns; ++column) {
      if (column > 0) line += "  ";
      const std::string cell = column < rows[row_index].size() ? rows[row_index][column] : "";
      line += centered(cell, widths[column]);
    }
    if (*right != '\0') {
      line += ' ';
      line += right;
    }
    out.push_back(std::move(line));
  }
  return out;
}

}  // namespace

std::vector<std::string> math_block_lines(std::string_view source) {
  const std::string trimmed = trim(std::string(source));
  if (trimmed.empty()) return {};
  for (const std::string_view command : {"\\frac", "\\dfrac", "\\tfrac"}) {
    if (!starts_with(trimmed, command)) continue;
    std::size_t index = command.size();
    std::string numerator;
    std::string denominator;
    if (read_group(trimmed, index, numerator) && read_group(trimmed, index, denominator) &&
        index == trimmed.size()) {
      return fraction_lines(numerator, denominator);
    }
  }
  if (starts_with(trimmed, "\\sqrt")) {
    std::size_t index = 5;
    while (index < trimmed.size() && trimmed[index] == ' ') ++index;
    std::string root;
    if (index < trimmed.size() && trimmed[index] == '[') {
      const auto close = trimmed.find(']', index + 1);
      if (close != std::string::npos) {
        root = trim(trimmed.substr(index + 1, close - index - 1));
        index = close + 1;
      }
    }
    std::string body;
    if (read_group(trimmed, index, body) && index == trimmed.size()) {
      return radical_lines(body, root);
    }
  }
  if (starts_with(trimmed, "\\begin{")) {
    const auto brace = trimmed.find('}');
    if (brace != std::string::npos) {
      const std::string env = trimmed.substr(7, brace - 7);
      const std::string close = "\\end{" + env + "}";
      if (ends_with(trimmed, close)) {
        const std::string body = trimmed.substr(brace + 1, trimmed.size() - close.size() - brace - 1);
        const auto lines = matrix_lines(env, body);
        if (!lines.empty()) return lines;
      }
    }
  }
  return {latex_to_unicode(trimmed)};
}

}  // namespace booth
