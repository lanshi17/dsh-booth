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

void erase_glyph(std::string& value) {
  if (value.empty()) return;
  std::size_t at = value.size() - 1;
  while (at > 0 && (static_cast<unsigned char>(value[at]) & 0xC0) == 0x80) --at;
  value.erase(at);
}

}  // namespace

Editor::Editor(ftxui::ScreenInteractive& screen, ControlClient& control, Listing listing,
               std::string provider, bool create)
    : screen_(screen), control_(control), rows_(std::move(listing.rows)),
      notice_(std::move(listing.error)), notice_error_(!notice_.empty()) {
  if (!provider.empty() || create) open_form(std::move(provider), create);
}

void Editor::reload() {
  const std::string current = rows_.empty() || selected_ >= rows_.size() ? std::string{} : rows_[selected_].name;
  Listing listing = list(control_);
  if (!listing.error.empty()) {
    notice_ = listing.error;
    notice_error_ = true;
    return;
  }
  rows_ = std::move(listing.rows);
  selected_ = 0;
  if (!current.empty()) {
    const auto found = std::find_if(rows_.begin(), rows_.end(),
                                    [&](const Row& row) { return row.name == current; });
    if (found != rows_.end()) selected_ = static_cast<std::size_t>(found - rows_.begin());
  }
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

void Editor::set_form_status(std::string text, bool error) {
  form_.status = std::move(text);
  form_.status_error = error;
}

void Editor::move_form(int step) {
  if (form_.fields.empty()) return;
  int index = static_cast<int>(form_.active);
  for (std::size_t guard = 0; guard < form_.fields.size(); ++guard) {
    index = (index + step + static_cast<int>(form_.fields.size())) % static_cast<int>(form_.fields.size());
    if (!form_.fields[static_cast<std::size_t>(index)].read_only) {
      form_.active = static_cast<std::size_t>(index);
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
  if (!field.read_only) erase_glyph(field.value);
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
    field.value += ch;
    return true;
  }
  return true;
}

bool Editor::OnEvent(ftxui::Event event) {
  if (mode_ == Mode::Form) return handle_form(std::move(event));
  if (event == ftxui::Event::Escape || event == ftxui::Event::Character('q')) {
    screen_.Exit();
    return true;
  }
  if (event == ftxui::Event::ArrowDown || event == ftxui::Event::Character('j')) {
    if (!rows_.empty()) selected_ = std::min(selected_ + 1, rows_.size() - 1);
    return true;
  }
  if (event == ftxui::Event::ArrowUp || event == ftxui::Event::Character('k')) {
    if (selected_ > 0) --selected_;
    return true;
  }
  if (event == ftxui::Event::Home || event == ftxui::Event::Character('g')) {
    selected_ = 0;
    return true;
  }
  if (event == ftxui::Event::End || event == ftxui::Event::Character('G')) {
    if (!rows_.empty()) selected_ = rows_.size() - 1;
    return true;
  }
  if (event == ftxui::Event::Return) {
    if (!rows_.empty()) open_form(rows_[selected_].name, false);
    return true;
  }
  if (event == ftxui::Event::Character('n')) {
    open_form({}, true);
    return true;
  }
  if (event == ftxui::Event::Character('r')) {
    reload();
    return true;
  }
  return true;
}

Element Editor::render_list() const {
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto panel = theme_color("panel", ftxui::Color::RGB(18, 18, 18));
  const auto hover = theme_color("hover", ftxui::Color::RGB(40, 40, 40));
  Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text(" booth-provider ") | ftxui::bold | ftxui::color(accent),
      ftxui::text("provider routes") | ftxui::dim,
      ftxui::filler(),
      ftxui::text(std::to_string(rows_.size()) + " ") | ftxui::dim,
  }));
  if (!notice_.empty()) {
    rows.push_back(ftxui::paragraph(notice_) |
                   ftxui::color(notice_error_ ? ftxui::Color::RGB(255, 95, 95) : dim));
  }
  rows.push_back(ftxui::text(""));
  if (rows_.empty()) {
    rows.push_back(ftxui::text(" no providers · press n to create one") | ftxui::dim);
  } else {
    constexpr std::size_t kRows = 14;
    std::size_t start = selected_ >= kRows / 2 ? selected_ - kRows / 2 : 0;
    if (start + kRows > rows_.size()) start = rows_.size() > kRows ? rows_.size() - kRows : 0;
    const std::size_t end = std::min(start + kRows, rows_.size());
    if (start > 0) rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    for (std::size_t index = start; index < end; ++index) {
      const auto& row = rows_[index];
      const bool active = index == selected_;
      Element line = ftxui::hbox({
          ftxui::text(active ? " › " : "   ") | ftxui::color(accent),
          ftxui::text(booth::ellipsize(row.label.empty() ? row.name : row.label, 26)) |
              (active ? ftxui::bold : ftxui::nothing),
          ftxui::text("  " + booth::ellipsize(row.detail, 48)) | ftxui::dim,
          ftxui::filler(),
      });
      if (active) line = line | ftxui::bgcolor(hover);
      rows.push_back(std::move(line));
    }
    if (end < rows_.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(rows_.size() - end) + " more") |
                     ftxui::dim);
    }
  }
  rows.push_back(ftxui::text(""));
  rows.push_back(ftxui::text(" ↑/↓ select · Enter edit · n new · r refresh · q quit") | ftxui::dim);
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 90) |
         ftxui::borderRounded | ftxui::center | ftxui::bgcolor(panel);
}

Element Editor::render_form() const {
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto panel = theme_color("panel", ftxui::Color::RGB(18, 18, 18));
  const auto hover = theme_color("hover", ftxui::Color::RGB(40, 40, 40));
  Elements rows;
  const std::string title = form_.create || form_.provider.empty()
                                ? "new provider"
                                : "provider · " + form_.provider;
  rows.push_back(ftxui::text(" " + title + " ") | ftxui::bold | ftxui::color(accent));
  if (!form_.status.empty()) {
    rows.push_back(ftxui::paragraph(form_.status) |
                   ftxui::color(form_.status_error ? ftxui::Color::RGB(255, 95, 95) : dim));
  }
  rows.push_back(ftxui::text(""));
  for (std::size_t index = 0; index < form_.fields.size(); ++index) {
    const auto& field = form_.fields[index];
    const bool active = index == form_.active;
    std::string value = field.value;
    if (field.secret && !value.empty()) value = std::string(std::min<std::size_t>(value.size(), 24), '*');
    if (value.empty()) value = field.hint;
    Element line = ftxui::hbox({
        ftxui::text(active ? " › " : "   ") | ftxui::color(accent),
        ftxui::text(field.label + "  ") | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 14) | ftxui::color(dim),
        ftxui::text(value) | ftxui::color(field.read_only ? dim : (field.value.empty() ? dim : text)),
        ftxui::filler(),
    });
    if (active) line = line | ftxui::bgcolor(hover);
    rows.push_back(std::move(line));
  }
  rows.push_back(ftxui::text(""));
  rows.push_back(ftxui::text(" Tab/↑↓ field · Enter next/save · Ctrl+S save · Ctrl+F discover · Ctrl+X remove · Esc back") |
                 ftxui::dim);
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 90) |
         ftxui::borderRounded | ftxui::center | ftxui::bgcolor(panel);
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
