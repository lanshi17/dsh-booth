#include "interaction/link_picker.hpp"

#include <algorithm>
#include <cctype>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <ftxui/dom/elements.hpp>

#include "interaction/open_link.hpp"
#include "interaction/theme.hpp"

namespace booth {
namespace {

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kYellow (theme_color("yellow", ftxui::Color::RGB(255, 215, 0)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))

std::string lower(std::string_view text) {
  std::string out(text);
  for (char& ch : out) {
    ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  }
  return out;
}

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

bool starts_with_ci(std::string_view text, std::size_t at, std::string_view token) {
  if (at + token.size() > text.size()) return false;
  for (std::size_t index = 0; index < token.size(); ++index) {
    if (std::tolower(static_cast<unsigned char>(text[at + index])) !=
        std::tolower(static_cast<unsigned char>(token[index]))) {
      return false;
    }
  }
  return true;
}

bool word_boundary(std::string_view text, std::size_t at) {
  if (at == 0) return true;
  const auto previous = static_cast<unsigned char>(text[at - 1]);
  return !std::isalnum(previous) && previous != '_' && previous != '+' && previous != '-';
}

bool url_stop(char ch) {
  const auto byte = static_cast<unsigned char>(ch);
  return byte <= 0x20 || byte == 0x7f || ch == '<' || ch == '>' || ch == '"' || ch == '\'' ||
         ch == '`';
}

std::string trim_url(std::string url) {
  while (!url.empty()) {
    const char last = url.back();
    if (last == '.' || last == ',' || last == ';' || last == ':' || last == '!' ||
        last == '?' || last == ')' || last == ']' || last == '}' || last == '*') {
      url.pop_back();
    } else {
      break;
    }
  }
  return url;
}

struct Candidate {
  std::size_t entry{0};
  std::size_t position{0};
  std::string url;
};

void markdown_candidates(const std::string& text, std::size_t entry,
                         std::vector<Candidate>& out) {
  std::size_t at = 0;
  while ((at = text.find('[', at)) != std::string::npos) {
    const auto label_end = text.find("](", at + 1);
    if (label_end == std::string::npos) {
      ++at;
      continue;
    }
    const auto url_end = text.find(')', label_end + 2);
    if (url_end == std::string::npos) {
      at = label_end + 2;
      continue;
    }
    std::string url = trim(std::string_view(text).substr(label_end + 2, url_end - label_end - 2));
    if (!url.empty()) out.push_back(Candidate{entry, at, std::move(url)});
    at = url_end + 1;
  }
}

std::size_t find_scheme(const std::string& text, std::string_view scheme, std::size_t from) {
  for (const std::size_t limit = text.size(); from + scheme.size() <= limit; ++from) {
    if (starts_with_ci(text, from, scheme) && word_boundary(text, from)) return from;
  }
  return std::string::npos;
}

void bare_candidates(const std::string& text, std::size_t entry, std::vector<Candidate>& out) {
  static constexpr std::string_view kSchemes[] = {"http://", "https://", "mailto:"};
  std::size_t at = 0;
  while (at < text.size()) {
    std::size_t found = std::string::npos;
    for (const auto scheme : kSchemes) {
      const auto position = find_scheme(text, scheme, at);
      if (position != std::string::npos && (found == std::string::npos || position < found)) {
        found = position;
      }
    }
    if (found == std::string::npos) break;
    std::size_t end = found;
    while (end < text.size() && !url_stop(text[end])) ++end;
    std::string url = trim_url(text.substr(found, end - found));
    if (!url.empty()) out.push_back(Candidate{entry, found, std::move(url)});
    at = end;
  }
}

std::string host_of(std::string_view url) {
  const auto colon = url.find(':');
  if (colon == std::string_view::npos) return {};
  const std::string scheme = lower(url.substr(0, colon));
  std::string_view rest = url.substr(colon + 1);
  if (scheme == "mailto") {
    rest = rest.substr(0, rest.find('?'));
    return std::string(rest);
  }
  while (!rest.empty() && rest.front() == '/') rest.remove_prefix(1);
  const auto end = rest.find_first_of("/?#");
  rest = rest.substr(0, end);
  const auto at = rest.rfind('@');
  if (at != std::string_view::npos) rest.remove_prefix(at + 1);
  return std::string(rest);
}

}  // namespace

std::string link_hint(std::size_t index) {
  if (index < 26) return std::string(1, static_cast<char>('a' + index));
  if (index < 52) return std::string(1, static_cast<char>('A' + index - 26));
  return "?";
}

std::vector<TranscriptLink> collect_transcript_links(
    const std::vector<TranscriptEntry>& entries) {
  std::vector<Candidate> candidates;
  for (std::size_t index = 0; index < entries.size(); ++index) {
    const auto& entry = entries[index];
    if (entry.kind != EntryKind::User && entry.kind != EntryKind::Assistant) continue;
    markdown_candidates(entry.text, index, candidates);
    bare_candidates(entry.text, index, candidates);
  }
  std::stable_sort(candidates.begin(), candidates.end(),
                   [](const Candidate& left, const Candidate& right) {
                     if (left.entry != right.entry) return left.entry < right.entry;
                     return left.position < right.position;
                   });

  std::vector<TranscriptLink> links;
  for (const auto& candidate : candidates) {
    if (!is_openable_url(candidate.url)) continue;
    const bool seen = std::any_of(links.begin(), links.end(), [&](const TranscriptLink& link) {
      return link.url == candidate.url;
    });
    if (seen) continue;
    links.push_back(TranscriptLink{candidate.url, host_of(candidate.url),
                                   link_hint(links.size())});
  }
  return links;
}

bool LinkPicker::open(const AppState& state) {
  items_ = collect_transcript_links(state.entries);
  index_ = 0;
  visible_ = true;
  return !items_.empty();
}

void LinkPicker::close() { visible_ = false; }

bool LinkPicker::select_hint(char hint) {
  if (!visible_) return false;
  for (std::size_t index = 0; index < items_.size(); ++index) {
    if (items_[index].hint.size() == 1 && items_[index].hint[0] == hint) {
      index_ = index;
      return true;
    }
  }
  return false;
}

bool LinkPicker::activate(AppState& state) {
  if (!visible_ || items_.empty() || index_ >= items_.size()) return false;
  const std::string url = items_[index_].url;
  state.notice = (open_url(url) ? "opened " : "could not open ") + url;
  close();
  return true;
}

bool LinkPicker::key(ftxui::Event& event, AppState& state) {
  if (!visible_) return false;
  if (event == ftxui::Event::Escape) {
    close();
    return true;
  }
  if (items_.empty()) return true;
  if (index_ >= items_.size()) index_ = 0;
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    index_ = (index_ + items_.size() - 1) % items_.size();
    return true;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    index_ = (index_ + 1) % items_.size();
    return true;
  }
  if (event == ftxui::Event::Home) {
    index_ = 0;
    return true;
  }
  if (event == ftxui::Event::End) {
    index_ = items_.size() - 1;
    return true;
  }
  if (event == ftxui::Event::Return) {
    activate(state);
    return true;
  }
  if (event.is_character()) {
    const std::string input = event.input();
    if (input.size() == 1 && select_hint(input[0])) {
      activate(state);
      return true;
    }
  }
  return true;
}

ftxui::Element link_picker_view(const std::vector<TranscriptLink>& items, std::size_t index) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text(" links · " + std::to_string(items.size()) + " ") | ftxui::bold |
          ftxui::color(kAccent),
      ftxui::text("↑/↓ · Enter · a-z · Esc") | ftxui::dim,
  }));
  if (items.empty()) {
    rows.push_back(ftxui::text(" no links in the transcript") | ftxui::dim);
  } else {
    constexpr std::size_t kRows = 10;
    std::size_t start = index >= kRows / 2 ? index - kRows / 2 : 0;
    if (start + kRows > items.size()) {
      start = items.size() > kRows ? items.size() - kRows : 0;
    }
    const std::size_t end = std::min(start + kRows, items.size());
    if (start > 0) {
      rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    }
    for (std::size_t row = start; row < end; ++row) {
      const bool selected = row == index;
      std::string url = items[row].url;
      if (url.size() > 44) url = url.substr(0, 43) + "…";
      ftxui::Element line = ftxui::hbox({
          ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(items[row].hint + " ") | ftxui::bold | ftxui::color(kYellow),
          ftxui::text(items[row].host) | ftxui::color(kText),
          ftxui::text("  " + url) | ftxui::color(kDim),
          ftxui::filler(),
      });
      if (selected) line = line | ftxui::bgcolor(kPanel);
      rows.push_back(std::move(line));
    }
    if (end < items.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(items.size() - end) + " more") |
                     ftxui::dim);
    }
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

}  // namespace booth
