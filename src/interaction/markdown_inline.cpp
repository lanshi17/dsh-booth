#include "interaction/view.hpp"

#include "interaction/math.hpp"
#include "interaction/theme.hpp"

#include <cctype>
#include <sstream>
#include <string>
#include <string_view>
#include <utility>

#include <ftxui/dom/elements.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);

ftxui::Element inline_words(const std::string& text, bool dim) {
  ftxui::Elements parts;
  bool code = false;
  bool bold = false;
  bool italic = false;
  bool strike = false;
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
      if (italic) part = part | ftxui::italic;
      if (strike) part = part | ftxui::strikethrough;
      parts.push_back(std::move(part));
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
    }
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
  return ftxui::hflow(std::move(parts));
}

}  // namespace

ftxui::Element inline_words_view(const std::string& text, bool dim) {
  return inline_words(text, dim);
}

}  // namespace booth
