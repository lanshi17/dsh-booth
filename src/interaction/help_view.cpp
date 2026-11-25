#include "interaction/view.hpp"

#include <algorithm>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include "interaction/keymap.hpp"
#include "interaction/theme.hpp"

namespace booth {
namespace {

std::string hrule(std::size_t count) {
  std::string out;
  for (std::size_t index = 0; index < count; ++index) out += "─";
  return out;
}

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kLine (theme_color("line", ftxui::Color::RGB(72, 72, 72)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))

}  // namespace

std::vector<HelpLine> help_lines(const KeyBindings& keys) {
  return {
      {"Enter send · Alt+Enter newline · ↑/↓ history"},
      {"Tab word completion · " + chord_label(keys.editor) + " editor · " +
          chord_label(keys.cancel) + " cancel · " + chord_label(keys.quit) + " quit"},
      {"Ctrl+Z undo · Alt+Z redo · Ctrl/Alt+←→ word"},
      {"Ctrl+W/U/K/Y kill · Alt+Y yank-pop · " + chord_label(keys.dequeue) + " edits queue"},
      {chord_label(keys.history) + " history · " + chord_label(keys.tools) + " tools · " +
          chord_label(keys.thinking) + " thinking"},
      {chord_label(keys.help) + " help · " + chord_label(keys.model) + " model · " +
          chord_label(keys.sessions) + " sessions · " + chord_label(keys.copy) + " copy"},
      {chord_label(keys.paste_image) + " image · " + chord_label(keys.links) + " links · " +
          chord_label(keys.model_next) + " next model · :tada: emoji · BOOTH_VIM for Vim"},
      {"!command runs local /bin/sh · output stays local"},
      {"$python runs Python locally; an expression prints its value"},
      {"^model mention applies a model · Alt+M picks one"},
      {"/ fuzzy commands · ↑/↓ wheel · Tab · Enter · Esc"},
      {chord_label(keys.page_up) + "/" + chord_label(keys.page_down) + " or wheel browse transcript"},
      {"Local while running: /theme /notify /copy /clear /export"},
      {"  /status /context /stats /effort /queue /help /quit"},
      {"/theme [key=#rrggbb] reload or set a live color"},
      {"/notify [on|off] terminal attention alerts"},
      {"/links transcript URLs · Alt+L · a-z hints"},
      {"/mouse [on|off] tmux scrollback vs app mouse"},
      {"/status phase, session and model"},
      {"/model auto|<id>"},
      {"/effort [level|next] · Shift+Tab · /context"},
      {"/stats transcript counters"},
      {"/copy [answer|prompt] · /clear clear transcript"},
      {"/retry last prompt · /export markdown transcript"},
      {"/sessions resume · /new fresh · /close stop"},
      {"/skills list dsh skills · /providers (or /setup) pick a provider"},
      {"/settings toggle thinking, tools, notifications, mouse and Vim"},
      {"/todo [copy|export] · /tools counts · /usage context and stats"},
      {"/git branch and working-tree counts"},
      {"/pin /unpin the active session in the resume list"},
      {"/session /acp /cancel · /find · /attach <path> · /detach"},
      {"/help this card · /quit leave"},
      {".booth/commands/*.md become /name prompt templates"},
      {"Aliases: /resume /models /reasoning /hotkeys /keys /exit /q", true},
  };
}

int help_visible_lines() {
  const int rows = std::max(12, ftxui::Terminal::Size().dimy);
  return std::max(4, rows - 7);
}

std::vector<HelpLine> help_lines() { return help_lines(KeyBindings{}); }

int help_max_offset() {
  const int total = static_cast<int>(help_lines().size());
  return std::max(0, total - std::min(total, help_visible_lines()));
}

ftxui::Element help_view(int offset, const KeyBindings& keys, MouseRouter* mouse) {
  const auto lines = help_lines(keys);
  const int total = static_cast<int>(lines.size());
  const int visible = std::min(total, help_visible_lines());
  const int first = std::clamp(offset, 0, std::max(0, total - visible));

  ftxui::Elements body;
  body.push_back(ftxui::text("booth — keys and commands") | ftxui::bold | ftxui::color(kAccent));
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  for (int index = first; index < first + visible; ++index) {
    ftxui::Element row = ftxui::text(lines[static_cast<std::size_t>(index)].text);
    if (lines[static_cast<std::size_t>(index)].dim) row = std::move(row) | ftxui::dim;
    body.push_back(std::move(row));
  }
  const std::string position = std::to_string(first + 1) + "-" +
                               std::to_string(first + visible) + " / " + std::to_string(total);
  body.push_back(ftxui::hbox({
      ftxui::text(total > visible ? "↑/↓ scroll · Esc/q close" : "Esc/q close") | ftxui::dim,
      ftxui::filler(),
      ftxui::text(position) | ftxui::dim,
  }));
  auto card = ftxui::hbox({ftxui::text(" "), ftxui::vbox(std::move(body)), ftxui::filler()}) |
              ftxui::bgcolor(kPanel);
  if (mouse != nullptr) card = mouse->zone(std::move(card), MouseSurface::Help, 0);
  return card;
}


}  // namespace booth
