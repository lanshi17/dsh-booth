#include "interaction/view.hpp"

#include "interaction/math.hpp"
#include "interaction/markdown_util.hpp"
#include "interaction/theme.hpp"

#include <algorithm>
#include <cctype>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/string.hpp>
#include <ftxui/screen/terminal.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);

bool starts_with(std::string_view text, std::string_view prefix) {
  return text.size() >= prefix.size() && text.compare(0, prefix.size(), prefix) == 0;
}

bool is_html_tag(const std::string& inner) {
  std::size_t index = !inner.empty() && inner.front() == '/' ? 1 : 0;
  if (index >= inner.size() || !std::isalpha(static_cast<unsigned char>(inner[index]))) {
    return false;
  }
  while (index < inner.size() &&
         (std::isalnum(static_cast<unsigned char>(inner[index])) || inner[index] == '-' ||
          inner[index] == '_')) {
    ++index;
  }
  return index == inner.size() || inner[index] == ' ' || inner[index] == '\t' ||
         inner[index] == '/';
}

bool is_autolink_target(const std::string& text) {
  return starts_with(text, "http://") || starts_with(text, "https://") ||
         starts_with(text, "ftp://") || starts_with(text, "www.") ||
         starts_with(text, "mailto:");
}

bool is_cjk_opening(const std::string& glyph) {
  static const std::string kOpening = "（【《「『〈〔｛“‘";
  return !glyph.empty() && kOpening.find(glyph) != std::string::npos;
}

bool is_cjk_closing(const std::string& glyph) {
  static const std::string kClosing = "。，、；：！？）】》」』〉〕｝”’…—～·";
  return !glyph.empty() && kClosing.find(glyph) != std::string::npos;
}

struct InlinePiece {
  ftxui::Element element;
  int width{0};
  bool space{false};
};

// TerminalOutput sizes its frame from the document's min_y, but an hflow
// wrapped paragraph reports one row until it is given a width. Laying the
// rows out here keeps the reported height equal to the drawn height, so the
// tail of a wrapped answer is not clipped away.
ftxui::Element layout_inline(const std::vector<InlinePiece>& pieces, int width) {
  if (width <= 0) width = std::max(20, ftxui::Terminal::Size().dimx - 3);
  ftxui::Elements rows;
  ftxui::Elements row;
  int used = 0;
  bool pending_space = false;
  ftxui::Element pending_element;
  const auto flush_row = [&] {
    if (!row.empty()) {
      rows.push_back(ftxui::hbox(std::move(row)));
      row.clear();
    }
    used = 0;
  };
  for (const auto& piece : pieces) {
    if (piece.space) {
      if (used > 0) {
        pending_space = true;
        pending_element = piece.element;
      }
      continue;
    }
    const int gap = pending_space ? 1 : 0;
    if (used > 0 && used + gap + piece.width > width) {
      flush_row();
      pending_space = false;
    }
    if (pending_space) {
      row.push_back(pending_element ? pending_element : ftxui::text(" "));
      pending_element.reset();
      ++used;
      pending_space = false;
    }
    row.push_back(piece.element);
    used += std::max(0, piece.width);
  }
  flush_row();
  if (rows.empty()) rows.push_back(ftxui::text(""));
  return ftxui::vbox(std::move(rows));
}

ftxui::Element inline_words(const std::string& raw, bool dim, int width) {
  const std::string text = decode_html_entities(raw);
  std::vector<InlinePiece> pieces;
  bool code = false;
  bool bold = false;
  bool italic = false;
  bool strike = false;
  std::string segment;
  const auto styled = [&](ftxui::Element part) {
    if (code) {
      part = part | ftxui::color(kGreen);
    } else if (dim) {
      part = part | ftxui::dim;
    }
    if (bold) part = part | ftxui::bold;
    if (italic) part = part | ftxui::italic;
    if (strike) part = part | ftxui::strikethrough;
    return part;
  };
  const auto push = [&](ftxui::Element element, int cells) {
    pieces.push_back(InlinePiece{std::move(element), cells, false});
  };
  const auto push_space = [&] {
    if (pieces.empty() || pieces.back().space) return;
    pieces.push_back(InlinePiece{styled(ftxui::text(" ")), 1, true});
  };
  const auto for_each_chunk = [&](const std::string& value, auto&& emit) {
    if (width <= 0 || ftxui::string_width(value) <= width) {
      emit(value);
      return;
    }
    std::string chunk;
    int used = 0;
    for (const auto& glyph : ftxui::Utf8ToGlyphs(value)) {
      if (glyph.empty()) continue;
      const int cells = ftxui::string_width(glyph);
      if (used > 0 && used + cells > width) {
        emit(chunk);
        chunk.clear();
        used = 0;
      }
      chunk += glyph;
      used += cells;
    }
    if (!chunk.empty()) emit(chunk);
  };
  // CJK text has no spaces, so a whole sentence would be one unbreakable
  // element. One element per wide glyph keeps every line inside the width;
  // closing punctuation stays attached to the glyph before it.
  const auto push_unit = [&](const std::string& unit) {
    const int cells = ftxui::string_width(unit);
    if (width <= 0 || cells <= width) {
      push(styled(ftxui::text(unit)), cells);
      return;
    }
    std::string chunk;
    int used = 0;
    for (const auto& glyph : ftxui::Utf8ToGlyphs(unit)) {
      if (glyph.empty()) continue;
      const int glyph_cells = ftxui::string_width(glyph);
      if (used > 0 && used + glyph_cells > width) {
        push(styled(ftxui::text(chunk)), used);
        chunk.clear();
        used = 0;
      }
      chunk += glyph;
      used += glyph_cells;
    }
    if (!chunk.empty()) push(styled(ftxui::text(chunk)), used);
  };
  const auto push_token = [&](const std::string& token) {
    std::vector<std::string> units;
    std::string opening;
    bool last_wide = false;
    for (const auto& glyph : ftxui::Utf8ToGlyphs(token)) {
      if (glyph.empty()) continue;
      const int cells = ftxui::string_width(glyph);
      if (cells == 0) {
        if (!units.empty()) units.back() += glyph;
        continue;
      }
      const bool wide = cells >= 2;
      if (wide && is_cjk_opening(glyph)) {
        opening += glyph;
        continue;
      }
      if (wide && is_cjk_closing(glyph) && !units.empty()) {
        units.back() += glyph;
        last_wide = false;
        continue;
      }
      if (wide) {
        units.push_back(opening + glyph);
        opening.clear();
        last_wide = true;
      } else if (!units.empty() && !last_wide) {
        units.back() += glyph;
      } else {
        units.push_back(opening + glyph);
        opening.clear();
        last_wide = false;
      }
    }
    if (!opening.empty()) {
      if (!units.empty()) {
        units.back() += opening;
      } else {
        units.push_back(opening);
      }
    }
    for (const auto& unit : units) push_unit(unit);
  };
  const auto flush = [&] {
    for (std::size_t index = 0; index < segment.size();) {
      if (std::isspace(static_cast<unsigned char>(segment[index]))) {
        push_space();
        ++index;
        continue;
      }
      std::size_t end = index;
      while (end < segment.size() &&
             !std::isspace(static_cast<unsigned char>(segment[end]))) {
        ++end;
      }
      const std::string token = segment.substr(index, end - index);
      index = end;
      if (!code) {
        std::string url = token;
        std::string suffix;
        if (url.size() >= 2 && url.front() == '<' && url.back() == '>') {
          url = url.substr(1, url.size() - 2);
        } else {
          while (!url.empty() && std::string(".,;:!?").find(url.back()) != std::string::npos) {
            suffix.insert(suffix.begin(), url.back());
            url.pop_back();
          }
        }
        if (is_autolink_target(url)) {
          const std::string href = starts_with(url, "www.") ? "https://" + url : url;
          for_each_chunk(url, [&](const std::string& part) {
            ftxui::Element label =
                ftxui::text(part) | ftxui::color(kAccent) | ftxui::underlined;
            if (bold) label = label | ftxui::bold;
            if (italic) label = label | ftxui::italic;
            if (strike) label = label | ftxui::strikethrough;
            if (dim) label = label | ftxui::dim;
            push(ftxui::hyperlink(href, std::move(label)), ftxui::string_width(part));
          });
          if (!suffix.empty()) push(styled(ftxui::text(suffix)), ftxui::string_width(suffix));
          continue;
        }
      }
      push_token(token);
    }
    segment.clear();
  };
  for (std::size_t index = 0; index < text.size();) {
    if (!code) {
      if (text[index] == '\\' && index + 1 < text.size() && text[index + 1] == '$') {
        segment += '$';
        index += 2;
        continue;
      }
      if (text[index] == '\\' && index + 1 < text.size() &&
          (text[index + 1] == '*' || text[index + 1] == '_' || text[index + 1] == '~')) {
        segment += text[index + 1];
        index += 2;
        continue;
      }
      if (const auto span = math_span_at(text, index)) {
        segment += span->text;
        index = span->end;
        continue;
      }
      if (text[index] == '\\' && index + 1 < text.size() &&
          std::ispunct(static_cast<unsigned char>(text[index + 1]))) {
        segment += text[index + 1];
        index += 2;
        continue;
      }
    }
    if (!code && text[index] == '!' && index + 1 < text.size() && text[index + 1] == '[') {
      const auto label_end = text.find("](", index + 1);
      const auto url_end = label_end == std::string::npos
                               ? std::string::npos
                               : link_end(text, label_end + 2);
      if (label_end != std::string::npos && url_end != std::string::npos) {
        flush();
        const std::string label = text.substr(index + 2, label_end - index - 2);
        std::string url = text.substr(label_end + 2, url_end - label_end - 2);
        const auto title = url.find_first_of(" \t");
        if (title != std::string::npos) url = url.substr(0, title);
        if (url.size() >= 2 && url.front() == '<' && url.back() == '>') {
          url = url.substr(1, url.size() - 2);
        }
        const std::string shown = label.empty() ? "[image]" : "[image: " + label + "]";
        ftxui::Element image = ftxui::text(shown) | ftxui::color(kDim);
        if (!url.empty()) image = ftxui::hyperlink(url, std::move(image));
        push(std::move(image), ftxui::string_width(shown));
        index = url_end + 1;
        continue;
      }
    }
    if (!code && text[index] == '<') {
      const auto close = text.find('>', index + 1);
      if (close != std::string::npos) {
        const std::string inner = text.substr(index + 1, close - index - 1);
        if (inner.find('@') != std::string::npos && inner.find(' ') == std::string::npos &&
            inner.find("://") == std::string::npos) {
          flush();
          ftxui::Element link =
              ftxui::text(inner) | ftxui::color(kAccent) | ftxui::underlined;
          if (bold) link = link | ftxui::bold;
          if (italic) link = link | ftxui::italic;
          if (strike) link = link | ftxui::strikethrough;
          if (dim) link = link | ftxui::dim;
          push(ftxui::hyperlink("mailto:" + inner, std::move(link)), ftxui::string_width(inner));
          index = close + 1;
          continue;
        }
        if (is_html_tag(inner)) {
          if (inner == "br" || inner == "/br" || inner == "br/" || inner == "/br/") {
            segment += ' ';
          }
          index = close + 1;
          continue;
        }
      }
    }
    if (!code && text[index] == '[') {
      const auto label_end = text.find("](", index);
      const auto url_end = label_end == std::string::npos
                               ? std::string::npos
                               : link_end(text, label_end + 2);
      if (label_end != std::string::npos && url_end != std::string::npos) {
        flush();
        const std::string label = text.substr(index + 1, label_end - index - 1);
        std::string url = text.substr(label_end + 2, url_end - label_end - 2);
        const auto title = url.find_first_of(" \t");
        if (title != std::string::npos) url = url.substr(0, title);
        if (url.size() >= 2 && url.front() == '<' && url.back() == '>') {
          url = url.substr(1, url.size() - 2);
        }
        for_each_chunk(label, [&](const std::string& part) {
          ftxui::Element link =
              ftxui::text(part) | ftxui::color(kAccent) | ftxui::underlined;
          if (bold) link = link | ftxui::bold;
          if (italic) link = link | ftxui::italic;
          if (strike) link = link | ftxui::strikethrough;
          if (dim) link = link | ftxui::dim;
          push(ftxui::hyperlink(url, std::move(link)), ftxui::string_width(part));
        });
        index = url_end + 1;
        continue;
      }
    }
    if (!code &&
        (text.compare(index, 3, "***") == 0 || text.compare(index, 3, "___") == 0)) {
      flush();
      bold = !bold;
      italic = !italic;
      index += 3;
      continue;
    }
    if (!code && (text.compare(index, 2, "**") == 0 || text.compare(index, 2, "__") == 0)) {
      flush();
      bold = !bold;
      index += 2;
      continue;
    }
    if (!code && text.compare(index, 2, "~~") == 0) {
      flush();
      strike = !strike;
      index += 2;
      continue;
    }
    if (!code && text[index] == '*') {
      flush();
      italic = !italic;
      ++index;
      continue;
    }
    if (!code && text[index] == '_') {
      const bool word_before =
          index > 0 && (std::isalnum(static_cast<unsigned char>(text[index - 1])) || text[index - 1] == '_');
      const bool word_after =
          index + 1 < text.size() &&
          (std::isalnum(static_cast<unsigned char>(text[index + 1])) || text[index + 1] == '_');
      if (word_before && word_after) {
        segment += '_';
        ++index;
        continue;
      }
      flush();
      italic = !italic;
      ++index;
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
  return layout_inline(pieces, width);
}

}  // namespace

ftxui::Element inline_words_view(const std::string& text, bool dim, int width) {
  return inline_words(text, dim, width);
}

}  // namespace booth
