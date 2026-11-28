#include "provider/editor.hpp"

#include <algorithm>
#include <cctype>
#include <iostream>
#include <string_view>
#include <utility>

#include <ftxui/component/component.hpp>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth::provider {
namespace {

using ftxui::Element;
using ftxui::Elements;

std::string lower_ascii(std::string_view text) {
  std::string out(text);
  for (char& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

void erase_glyph(std::string& value) {
  if (value.empty()) return;
  std::size_t at = value.size() - 1;
  while (at > 0 && (static_cast<unsigned char>(value[at]) & 0xC0) == 0x80) --at;
  value.erase(at);
}

std::size_t previous_glyph(std::string_view value, std::size_t cursor) {
  if (cursor == 0) return 0;
  std::size_t at = cursor - 1;
  while (at > 0 && (static_cast<unsigned char>(value[at]) & 0xC0) == 0x80) --at;
  return at;
}

std::size_t next_glyph(std::string_view value, std::size_t cursor) {
  if (cursor >= value.size()) return value.size();
  std::size_t at = cursor + 1;
  while (at < value.size() && (static_cast<unsigned char>(value[at]) & 0xC0) == 0x80) ++at;
  return at;
}

void erase_before(std::string& value, std::size_t& cursor) {
  if (cursor == 0) return;
  const std::size_t start = previous_glyph(value, cursor);
  value.erase(start, cursor - start);
  cursor = start;
}

void erase_at(std::string& value, std::size_t cursor) {
  if (cursor >= value.size()) return;
  value.erase(cursor, next_glyph(value, cursor) - cursor);
}

void erase_word(std::string& value, std::size_t& cursor) {
  std::size_t end = cursor;
  while (end > 0 && std::isspace(static_cast<unsigned char>(value[end - 1])) != 0) --end;
  while (end > 0 && std::isspace(static_cast<unsigned char>(value[end - 1])) == 0) --end;
  value.erase(end, cursor - end);
  cursor = end;
}

}  // namespace

Editor::Editor(ftxui::ScreenInteractive& screen, ControlClient& control, Listing listing,
               std::string provider, bool create)
    : screen_(screen), control_(control), rows_(std::move(listing.rows)),
      notice_(std::move(listing.error)), notice_error_(!notice_.empty()) {
  if (!provider.empty() || create) open_form(std::move(provider), create);
}

void Editor::clamp_selection() {
  const auto matches = visible_rows();
  if (matches.empty()) {
    selected_ = 0;
  } else if (selected_ >= matches.size()) {
    selected_ = matches.size() - 1;
  }
}

std::vector<std::size_t> Editor::visible_rows() const {
  std::vector<std::size_t> out;
  const std::string needle = lower_ascii(query_);
  for (std::size_t index = 0; index < rows_.size(); ++index) {
    if (needle.empty()) {
      out.push_back(index);
      continue;
    }
    const auto& row = rows_[index];
    const std::string haystack = lower_ascii(row.name + " " + row.label);
    if (haystack.find(needle) != std::string::npos) out.push_back(index);
  }
  return out;
}

void Editor::reload() {
  const auto before = visible_rows();
  const std::string current = before.empty() || selected_ >= before.size()
                                  ? std::string{}
                                  : rows_[before[selected_]].name;
  Listing listing = list(control_);
  if (!listing.error.empty()) {
    notice_ = listing.error;
    notice_error_ = true;
    return;
  }
  rows_ = std::move(listing.rows);
  const auto after = visible_rows();
  selected_ = 0;
  if (!current.empty()) {
    for (std::size_t index = 0; index < after.size(); ++index) {
      if (rows_[after[index]].name == current) {
        selected_ = index;
        break;
      }
    }
  }
  clamp_selection();
}

void Editor::open_form(std::string provider, bool create) {
  auto loaded = load(control_, std::move(provider), create);
  if (!loaded) {
    mode_ = Mode::List;
    notice_ = loaded.error().detail;
    notice_error_ = true;
    return;
  }
  form_ = std::move(*loaded);
  mode_ = Mode::Form;
  notice_.clear();
  notice_error_ = false;
}

void Editor::move_form(int step) {
  if (form_.fields.empty()) return;
  int index = static_cast<int>(form_.active);
  for (std::size_t guard = 0; guard < form_.fields.size(); ++guard) {
    index = (index + step + static_cast<int>(form_.fields.size())) % static_cast<int>(form_.fields.size());
    if (!form_.fields[static_cast<std::size_t>(index)].read_only) {
      form_.active = static_cast<std::size_t>(index);
      form_.cursor = form_.fields[form_.active].value.size();
      return;
    }
  }
}

bool Editor::last_editable() const {
  for (std::size_t index = form_.fields.size(); index > 0; --index) {
    if (!form_.fields[index - 1].read_only) return form_.active == index - 1;
  }
  return true;
}

void Editor::erase_form_last() {
  if (form_.fields.empty()) return;
  auto& field = form_.fields[form_.active];
  if (!field.read_only) erase_before(field.value, form_.cursor);
}

bool Editor::handle_form(ftxui::Event event) {
  if (event == ftxui::Event::Escape) {
    mode_ = Mode::List;
    return true;
  }
  if (event == ftxui::Event::Tab || event == ftxui::Event::ArrowDown) {
    move_form(1);
    return true;
  }
  if (event == ftxui::Event::TabReverse || event == ftxui::Event::ArrowUp) {
    move_form(-1);
    return true;
  }
  if (event == ftxui::Event::ArrowLeft) {
    if (!form_.fields.empty()) form_.cursor = previous_glyph(form_.fields[form_.active].value, form_.cursor);
    return true;
  }
  if (event == ftxui::Event::ArrowRight) {
    if (!form_.fields.empty()) form_.cursor = next_glyph(form_.fields[form_.active].value, form_.cursor);
    return true;
  }
  if (event == ftxui::Event::Home) {
    form_.cursor = 0;
    return true;
  }
  if (event == ftxui::Event::End) {
    if (!form_.fields.empty()) form_.cursor = form_.fields[form_.active].value.size();
    return true;
  }
  if (event == ftxui::Event::Delete) {
    if (!form_.fields.empty()) erase_at(form_.fields[form_.active].value, form_.cursor);
    return true;
  }
  if (event == ftxui::Event::CtrlU) {
    if (!form_.fields.empty()) {
      auto& field = form_.fields[form_.active];
      field.value.erase(0, form_.cursor);
      form_.cursor = 0;
    }
    return true;
  }
  if (event == ftxui::Event::CtrlW) {
    if (!form_.fields.empty()) erase_word(form_.fields[form_.active].value, form_.cursor);
    return true;
  }
  if (event == ftxui::Event::Backspace) {
    erase_form_last();
    return true;
  }
  if (event == ftxui::Event::Return) {
    if (last_editable()) {
      std::string text;
      if (save(control_, form_, text)) {
        mode_ = Mode::List;
        notice_ = std::move(text);
        notice_error_ = false;
        reload();
      }
    } else {
      move_form(1);
    }
    return true;
  }
  if (event == ftxui::Event::CtrlS) {
    std::string text;
    if (save(control_, form_, text)) {
      mode_ = Mode::List;
      notice_ = std::move(text);
      notice_error_ = false;
      reload();
    }
    return true;
  }
  if (event == ftxui::Event::CtrlF) {
    discover(control_, form_);
    return true;
  }
  if (event == ftxui::Event::CtrlX) {
    std::string text;
    if (remove(control_, form_, text)) {
      mode_ = Mode::List;
      notice_ = std::move(text);
      notice_error_ = false;
      reload();
    }
    return true;
  }
  if (event.is_character()) {
    if (form_.fields.empty()) return true;
    auto& field = form_.fields[form_.active];
    if (field.read_only) return true;
    const std::string ch = event.character();
    if (ch.size() == 1 && (static_cast<unsigned char>(ch[0]) < 0x20 || ch[0] == 0x7f)) return true;
    field.value.insert(form_.cursor, ch);
    form_.cursor += ch.size();
    return true;
  }
  return true;
}

bool Editor::OnEvent(ftxui::Event event) {
  if (mode_ == Mode::Form) return handle_form(std::move(event));
  const auto matches = visible_rows();
  if (event == ftxui::Event::Escape) {
    screen_.Exit();
    return true;
  }
  if (event == ftxui::Event::CtrlN) {
    open_form({}, true);
    return true;
  }
  if (event == ftxui::Event::CtrlR) {
    reload();
    return true;
  }
  if (event == ftxui::Event::CtrlU) {
    query_.clear();
    selected_ = 0;
    return true;
  }
  if (event == ftxui::Event::ArrowDown) {
    if (!matches.empty()) selected_ = std::min(selected_ + 1, matches.size() - 1);
    return true;
  }
  if (event == ftxui::Event::ArrowUp) {
    if (selected_ > 0) --selected_;
    return true;
  }
  if (event == ftxui::Event::PageDown) {
    if (!matches.empty()) selected_ = std::min(selected_ + 10, matches.size() - 1);
    return true;
  }
  if (event == ftxui::Event::PageUp) {
    selected_ = selected_ > 10 ? selected_ - 10 : 0;
    return true;
  }
  if (event == ftxui::Event::Home) {
    selected_ = 0;
    return true;
  }
  if (event == ftxui::Event::End) {
    if (!matches.empty()) selected_ = matches.size() - 1;
    return true;
  }
  if (event == ftxui::Event::Return) {
    if (!matches.empty()) open_form(rows_[matches[selected_]].name, false);
    return true;
  }
  if (event == ftxui::Event::Backspace) {
    if (!query_.empty()) {
      erase_glyph(query_);
      clamp_selection();
    }
    return true;
  }
  if (event.is_character()) {
    const std::string ch = event.character();
    if (ch.size() == 1 && (static_cast<unsigned char>(ch[0]) < 0x20 || ch[0] == 0x7f)) return true;
    query_ += ch;
    selected_ = 0;
    return true;
  }
  return true;
}

Element Editor::render_list() const {
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto panel = theme_color("panel", ftxui::Color::RGB(18, 18, 18));
  const auto hover = theme_color("hover", ftxui::Color::RGB(40, 40, 40));
  const auto green = theme_color("green", ftxui::Color::RGB(80, 220, 120));
  const auto matches = visible_rows();
  constexpr std::size_t kRows = 10;
  std::size_t start = selected_ >= kRows / 2 ? selected_ - kRows / 2 : 0;
  if (start + kRows > matches.size()) start = matches.size() > kRows ? matches.size() - kRows : 0;
  const std::size_t end = std::min(start + kRows, matches.size());

  Elements box;
  box.push_back(ftxui::hbox({
      ftxui::text(" Select provider to edit ") | ftxui::bold | ftxui::color(accent),
      ftxui::filler(),
      ftxui::text(std::to_string(matches.size()) + "/" + std::to_string(rows_.size()) + " ") |
          ftxui::dim,
  }));
  if (!query_.empty()) {
    box.push_back(ftxui::text(" Search: " + booth::ellipsize(query_, 70)) | ftxui::color(text));
  }
  if (matches.empty()) {
    box.push_back(ftxui::text(rows_.empty() ? " No providers · ^N to create one"
                                            : " No matching providers") |
                  ftxui::dim);
  } else {
    for (std::size_t index = start; index < end; ++index) {
      const auto& row = rows_[matches[index]];
      const bool active = index == selected_;
      std::string status;
      if (row.has_key) status = " ✓ key";
      else if (row.configured) status = " ✓ configured";
      Element line = ftxui::hbox({
          ftxui::text(active ? " ❯ " : "   ") | ftxui::color(accent),
          ftxui::text(booth::ellipsize(row.label.empty() ? row.name : row.label, 56)) |
              (active ? ftxui::bold | ftxui::color(accent) : ftxui::nothing),
          ftxui::text(status) | ftxui::color(green),
          ftxui::filler(),
      });
      if (active) line = line | ftxui::bgcolor(hover);
      box.push_back(std::move(line));
    }
    if (query_.empty()) box.push_back(ftxui::text(" Type to search") | ftxui::dim);
  }

  Elements page;
  page.push_back(ftxui::hbox({
      ftxui::text(" booth-provider ") | ftxui::bold | ftxui::color(accent),
      ftxui::text("dsh provider routes") | ftxui::dim,
      ftxui::filler(),
  }));
  page.push_back(ftxui::text(" Configure your providers") | ftxui::bold);
  page.push_back(ftxui::text(" Edit dsh provider routes. Press ⎋ when you're done.") | ftxui::dim);
  page.push_back(ftxui::text(" Pick a provider to edit — you can edit more than one.") | ftxui::dim);
  if (!notice_.empty()) {
    page.push_back(ftxui::paragraph(" " + notice_) |
                   ftxui::color(notice_error_ ? ftxui::Color::RGB(255, 95, 95) : dim));
  }
  page.push_back(ftxui::text(""));
  page.push_back(ftxui::vbox(std::move(box)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 92) |
                  ftxui::borderRounded | ftxui::bgcolor(panel));
  page.push_back(ftxui::text(" ↑/↓ select · ⏎ edit · ^N new · ^R refresh · ⎋ quit") | ftxui::dim);
  return ftxui::vbox(std::move(page)) | ftxui::hcenter;
}

Element Editor::render_form() const {
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto panel = theme_color("panel", ftxui::Color::RGB(18, 18, 18));
  const auto hover = theme_color("hover", ftxui::Color::RGB(40, 40, 40));
  Elements rows;
  const std::string title = form_.create || form_.provider.empty()
                                ? " new provider "
                                : " provider · " + form_.provider + " ";
  rows.push_back(ftxui::text(title) | ftxui::bold | ftxui::color(accent));
  if (!form_.status.empty()) {
    rows.push_back(ftxui::paragraph(" " + form_.status) |
                   ftxui::color(form_.status_error ? ftxui::Color::RGB(255, 95, 95) : dim));
  }
  rows.push_back(ftxui::text(""));
  for (std::size_t index = 0; index < form_.fields.size(); ++index) {
    const auto& field = form_.fields[index];
    const bool active = index == form_.active;
    Element value;
    if (field.secret && !field.value.empty()) {
      const std::size_t shown = std::min<std::size_t>(field.value.size(), 24);
      const std::size_t cursor = std::min(form_.cursor, shown);
      value = active
                  ? ftxui::hbox({ftxui::text(std::string(cursor, '*')) | ftxui::color(text),
                                 ftxui::text("▏") | ftxui::color(accent),
                                 ftxui::text(std::string(shown - cursor, '*')) | ftxui::color(text)})
                  : ftxui::text(std::string(shown, '*')) | ftxui::color(text);
    } else if (field.value.empty()) {
      value = active ? ftxui::hbox({ftxui::text("▏") | ftxui::color(accent),
                                    ftxui::text(field.hint) | ftxui::color(dim)})
                     : ftxui::text(field.hint) | ftxui::color(dim);
    } else if (active && !field.read_only) {
      constexpr std::size_t kWidth = 60;
      std::size_t start = 0;
      if (form_.cursor > kWidth / 2) start = form_.cursor - kWidth / 2;
      if (start + kWidth > field.value.size()) {
        start = field.value.size() > kWidth ? field.value.size() - kWidth : 0;
      }
      const std::size_t end = std::min(start + kWidth, field.value.size());
      std::string before = field.value.substr(start, form_.cursor - start);
      std::string after = field.value.substr(form_.cursor, end - form_.cursor);
      if (start > 0) before = "…" + before;
      if (end < field.value.size()) after += "…";
      value = ftxui::hbox({ftxui::text(before) | ftxui::color(text),
                           ftxui::text("▏") | ftxui::color(accent),
                           ftxui::text(after) | ftxui::color(text)});
    } else {
      value = ftxui::text(booth::ellipsize(field.value, 68)) | ftxui::color(text);
    }
    Element line = ftxui::hbox({
        ftxui::text(active ? " › " : "   ") | ftxui::color(accent),
        ftxui::text(field.label + "  ") | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 14) | ftxui::color(dim),
        std::move(value),
        ftxui::filler(),
    });
    if (active) line = line | ftxui::bgcolor(hover);
    rows.push_back(std::move(line));
  }
  rows.push_back(ftxui::text(""));
  return ftxui::vbox({
             ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 92) |
                 ftxui::borderRounded | ftxui::bgcolor(panel),
             ftxui::text(" Tab/↑↓ field · ←/→ cursor · ^S save · ^F discover · ^X remove · ⎋ back") |
                 ftxui::dim,
         }) |
         ftxui::hcenter;
}

ftxui::Element Editor::OnRender() {
  return mode_ == Mode::Form ? render_form() : render_list();
}

int run_editor(ControlClient& control, Listing listing, std::string provider, bool create) {
  auto screen = ftxui::ScreenInteractive::Fullscreen();
  screen.TrackMouse(true);
  auto editor = ftxui::Make<Editor>(screen, control, std::move(listing), std::move(provider), create);
  std::cout << "\x1b[2J\x1b[H" << std::flush;
  screen.Loop(editor);
  return 0;
}

}  // namespace booth::provider
