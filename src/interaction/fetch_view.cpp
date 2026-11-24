#include "interaction/fetch_view.hpp"

#include <ftxui/dom/elements.hpp>

#include <algorithm>
#include <string>
#include <vector>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {
namespace {

std::string url_host(const std::string& url) {
  std::size_t start = url.find("://");
  start = start == std::string::npos ? 0 : start + 3;
  const auto end = url.find_first_of("/?#", start);
  return url.substr(start, end == std::string::npos ? std::string::npos : end - start);
}

}  // namespace

std::string web_fetch_title(const WebFetchResult& result) {
  std::string title = "Web fetch";
  const std::string host = url_host(result.url);
  if (!host.empty()) title += " · " + host;
  if (!result.status.empty()) title += " · HTTP " + result.status;
  return title;
}

ftxui::Elements web_fetch_rows(const WebFetchResult& result, bool expanded,
                               std::size_t content_width) {
  const auto border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text("│ ") | ftxui::color(border),
      ftxui::hyperlink(result.url, ftxui::text(ellipsize(result.url, content_width))) |
          ftxui::dim | ftxui::flex,
      ftxui::text(" │") | ftxui::color(border),
  }));
  constexpr std::size_t kFetchLines = 12;
  const std::size_t limit =
      expanded ? result.body.size() : std::min(result.body.size(), kFetchLines);
  for (std::size_t index = 0; index < limit; ++index) {
    for (const auto& wrapped : wrap_cells(result.body[index], content_width)) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(wrapped) | ftxui::flex | ftxui::color(text),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (result.body.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text("No content") | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  } else if (!expanded && result.body.size() > limit) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text("… +" + std::to_string(result.body.size() - limit) +
                    " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  if (!result.footer.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text(result.footer) | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  return rows;
}

}  // namespace booth
