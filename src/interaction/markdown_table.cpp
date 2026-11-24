#include "interaction/markdown_table.hpp"

#include "interaction/markdown_util.hpp"
#include "interaction/math.hpp"

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/string.hpp>

#include <algorithm>
#include <cctype>
#include <string>
#include <string_view>
#include <vector>

namespace booth {

const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);

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

std::string clean_cell(std::string text);

std::string clean_cell(std::string text) {
  text = decode_html_entities(text);
  text = render_math_in_text(text);
  std::string out;
  for (std::size_t index = 0; index < text.size();) {
    if (text.compare(index, 2, "**") == 0 || text.compare(index, 2, "__") == 0 ||
        text.compare(index, 2, "~~") == 0) {
      index += 2;
      continue;
    }
    if (text[index] == '*' || text[index] == '`') {
      ++index;
      continue;
    }
    if (text[index] == '!' && index + 1 < text.size() && text[index + 1] == '[') {
      ++index;
      continue;
    }
    if (text[index] == '[') {
      const auto close = text.find("](", index);
      const auto end = close == std::string::npos ? std::string::npos
                                                  : text.find(')', close + 2);
      if (close != std::string::npos && end != std::string::npos) {
        out += text.substr(index + 1, close - index - 1);
        index = end + 1;
        continue;
      }
    }
    if (text[index] == '<') {
      const auto close = text.find('>', index);
      if (close != std::string::npos) {
        const std::string inner = text.substr(index + 1, close - index - 1);
        if (inner.rfind("http://", 0) == 0 || inner.rfind("https://", 0) == 0 ||
            inner.rfind("mailto:", 0) == 0) {
          out += inner;
          index = close + 1;
          continue;
        }
      }
    }
    if (text[index] == '_') {
      const bool before =
          index > 0 && (std::isalnum(static_cast<unsigned char>(text[index - 1])) || text[index - 1] == '_');
      const bool after =
          index + 1 < text.size() &&
          (std::isalnum(static_cast<unsigned char>(text[index + 1])) || text[index + 1] == '_');
      if (!(before && after)) {
        ++index;
        continue;
      }
    }
    out += text[index++];
  }
  return out;
}

TableCell styled_cell(std::string raw) {
  TableCell cell;
  const auto wrapped = [&](std::string_view open, std::string_view close) {
    return raw.size() >= open.size() + close.size() &&
           raw.compare(0, open.size(), open) == 0 &&
           raw.compare(raw.size() - close.size(), close.size(), close) == 0;
  };
  if (wrapped("**", "**")) {
    cell.bold = true;
    raw = raw.substr(2, raw.size() - 4);
  } else if (wrapped("__", "__")) {
    cell.bold = true;
    raw = raw.substr(2, raw.size() - 4);
  } else if (wrapped("`", "`")) {
    cell.code = true;
    raw = raw.substr(1, raw.size() - 2);
  } else if (wrapped("*", "*") || wrapped("_", "_")) {
    cell.italic = true;
    raw = raw.substr(1, raw.size() - 2);
  } else if (!raw.empty() && raw.front() == '[') {
    const auto close = raw.find("](");
    const auto end = close == std::string::npos ? std::string::npos : raw.find(')', close + 2);
    if (close != std::string::npos && end != std::string::npos) {
      cell.href = decode_html_entities(raw.substr(close + 2, end - close - 2));
      raw = raw.substr(1, close - 1);
    }
  } else if (raw.rfind("![", 0) == 0) {
    const auto close = raw.find("](");
    const auto end = close == std::string::npos ? std::string::npos : raw.find(')', close + 2);
    if (close != std::string::npos && end != std::string::npos) {
      raw = raw.substr(2, close - 2);
    }
  }
  cell.text = clean_cell(raw);
  return cell;
}

std::vector<TableCell> styled_cells(const std::vector<std::string>& cells) {
  std::vector<TableCell> out;
  out.reserve(cells.size());
  for (const auto& cell : cells) out.push_back(styled_cell(cell));
  return out;
}

bool is_table_start(const std::vector<std::string>& lines, std::size_t index) {
  return index + 1 < lines.size() && lines[index].find('|') != std::string::npos &&
         is_table_separator(lines[index + 1]);
}

std::vector<TableAlign> table_alignments(const std::string& line) {
  std::vector<TableAlign> aligns;
  for (const auto& cell : table_cells(line)) {
    const bool left = !cell.empty() && cell.front() == ':';
    const bool right = !cell.empty() && cell.back() == ':';
    aligns.push_back(left && right ? TableAlign::Center
                                   : right ? TableAlign::Right
                                           : TableAlign::Left);
  }
  return aligns;
}

int table_width(const std::string& text) { return ftxui::string_width(text); }

std::size_t glyph_end(const std::string& text, std::size_t index) {
  std::size_t end = index + 1;
  while (end < text.size() && (static_cast<unsigned char>(text[end]) & 0xC0) == 0x80) ++end;
  return end;
}

std::string rule(const std::string& glyph, int count) {
  std::string out;
  for (int index = 0; index < std::max(0, count); ++index) out += glyph;
  return out;
}

std::vector<std::string> wrap_cell(const std::string& text, int width) {
  if (width <= 0) return {text};
  std::vector<std::string> lines(1);
  int used = 0;
  std::size_t last_space = std::string::npos;
  for (std::size_t index = 0; index < text.size();) {
    const std::size_t end = glyph_end(text, index);
    const std::string glyph = text.substr(index, end - index);
    const int cells = table_width(glyph);
    if (used + cells > width && !lines.back().empty()) {
      if (last_space != std::string::npos) {
        std::string carry = lines.back().substr(last_space + 1);
        lines.back() = lines.back().substr(0, last_space);
        lines.push_back(std::move(carry));
        used = table_width(lines.back());
      } else {
        lines.emplace_back();
        used = 0;
      }
      last_space = std::string::npos;
      const auto space = lines.back().find_last_of(' ');
      if (space != std::string::npos) last_space = space;
    }
    lines.back() += glyph;
    used += cells;
    if (glyph == " ") last_space = lines.back().size() - 1;
    index = end;
  }
  return lines;
}

std::string pad_cell(const std::string& text, int width, TableAlign align) {
  const int padding = std::max(0, width - table_width(text));
  int left = 0;
  if (align == TableAlign::Right) left = padding;
  else if (align == TableAlign::Center) left = padding / 2;
  return std::string(static_cast<std::size_t>(left), ' ') + text +
         std::string(static_cast<std::size_t>(padding - left), ' ');
}

std::vector<int> fit_columns(const std::vector<int>& natural, int available) {
  std::vector<int> widths = natural;
  std::vector<int> minimums(natural.size(), 1);
  for (std::size_t index = 0; index < natural.size(); ++index) {
    minimums[index] = std::min(natural[index], 6);
  }
  int total = 0;
  for (const int width : widths) total += width;
  while (total > available) {
    std::size_t widest = 0;
    bool found = false;
    for (std::size_t index = 0; index < widths.size(); ++index) {
      if (widths[index] <= minimums[index]) continue;
      if (!found || widths[index] > widths[widest]) {
        widest = index;
        found = true;
      }
    }
    if (!found) break;
    --widths[widest];
    --total;
  }
  return widths;
}

ftxui::Element table_view(const std::vector<std::vector<TableCell>>& rows,
                          const std::vector<TableAlign>& aligns, int max_width) {
  const ftxui::Color border = ftxui::Color::RGB(72, 72, 72);
  std::size_t columns = 0;
  for (const auto& row : rows) columns = std::max(columns, row.size());
  if (columns == 0) return ftxui::text("");
  std::vector<int> natural(columns, 0);
  for (const auto& row : rows) {
    for (std::size_t column = 0; column < row.size(); ++column) {
      natural[column] = std::max(natural[column], table_width(row[column].text));
    }
  }
  const int overhead = static_cast<int>(3 * columns + 1);
  const int available = std::max(static_cast<int>(columns), max_width - overhead);
  const std::vector<int> widths = fit_columns(natural, available);
  const auto alignment = [&](std::size_t column) {
    return column < aligns.size() ? aligns[column] : TableAlign::Left;
  };

  ftxui::Elements body;
  const auto border_line = [&](const char* left, const char* mid, const char* right) {
    ftxui::Elements line;
    line.push_back(ftxui::text(left) | ftxui::color(border));
    for (std::size_t column = 0; column < columns; ++column) {
      line.push_back(ftxui::text(rule("─", widths[column] + 2)) | ftxui::color(border));
      line.push_back(ftxui::text(column + 1 == columns ? right : mid) | ftxui::color(border));
    }
    return ftxui::hbox(std::move(line));
  };
  const TableCell empty;
  const auto paint = [&](const std::vector<TableCell>& row, bool header) {
    std::vector<std::vector<std::string>> cells(columns);
    std::size_t height = 1;
    for (std::size_t column = 0; column < columns; ++column) {
      const TableCell& cell = column < row.size() ? row[column] : empty;
      cells[column] = wrap_cell(cell.text, widths[column]);
      height = std::max(height, cells[column].size());
    }
    for (std::size_t line_index = 0; line_index < height; ++line_index) {
      ftxui::Elements line;
      line.push_back(ftxui::text("│") | ftxui::color(border));
      for (std::size_t column = 0; column < columns; ++column) {
        const std::string& text =
            line_index < cells[column].size() ? cells[column][line_index] : "";
        const TableCell& cell = column < row.size() ? row[column] : empty;
        ftxui::Element part =
            ftxui::text(" " + pad_cell(text, widths[column], alignment(column)) + " ");
        if (cell.code) {
          part = part | ftxui::color(kGreen);
        } else {
          part = part | ftxui::color(kText);
        }
        if (header || cell.bold) part = part | ftxui::bold;
        if (cell.italic) part = part | ftxui::italic;
        if (!cell.href.empty()) part = ftxui::hyperlink(cell.href, std::move(part));
        line.push_back(std::move(part));
        line.push_back(ftxui::text("│") | ftxui::color(border));
      }
      body.push_back(ftxui::hbox(std::move(line)));
    }
  };

  body.push_back(border_line("┌", "┬", "┐"));
  paint(rows.front(), true);
  body.push_back(border_line("├", "┼", "┤"));
  for (std::size_t row = 1; row < rows.size(); ++row) {
    paint(rows[row], false);
    if (row + 1 < rows.size()) body.push_back(border_line("├", "┼", "┤"));
  }
  body.push_back(border_line("└", "┴", "┘"));
  return ftxui::vbox(std::move(body));
}

}  // namespace booth
