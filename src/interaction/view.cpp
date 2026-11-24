#include "interaction/view.hpp"
#include "interaction/context_gauge.hpp"
#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"
#include "interaction/thought_view.hpp"
#include "interaction/tool_diff.hpp"

#include <ftxui/screen/terminal.hpp>
#include <sys/ioctl.h>

#include <iostream>
#include <nlohmann/json.hpp>

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace booth {
namespace {

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kLine (theme_color("line", ftxui::Color::RGB(72, 72, 72)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kYellow (theme_color("yellow", ftxui::Color::RGB(255, 215, 0)))
#define kRed (theme_color("red", ftxui::Color::RGB(255, 95, 95)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))
#define kGit (theme_color("git", ftxui::Color::RGB(255, 175, 95)))
#define kGray (theme_color("gray", ftxui::Color::RGB(168, 168, 168)))
#define kToken (theme_color("token", ftxui::Color::RGB(95, 135, 175)))

std::string hrule(std::size_t count) {
  std::string out;
  for (std::size_t index = 0; index < count; ++index) out += "─";
  return out;
}

std::string compact_tokens(std::size_t value) {
  if (value >= 1000000) return std::to_string(value / 1000000) + "M";
  if (value >= 1000) return std::to_string(value / 1000) + "K";
  return std::to_string(value);
}

std::string run_command(const std::string& command) {
  std::string out;
  if (FILE* pipe = ::popen(command.c_str(), "r")) {
    char buffer[4096];
    while (std::fgets(buffer, sizeof(buffer), pipe) != nullptr) out += buffer;
    ::pclose(pipe);
  }
  return out;
}

std::string shell_quote(const std::string& value) {
  std::string out = "'";
  for (const char ch : value) {
    if (ch == '\'') {
      out += "'\\''";
    } else {
      out += ch;
    }
  }
  out += "'";
  return out;
}

std::string trim_line(std::string value) {
  while (!value.empty() && (value.back() == '\n' || value.back() == '\r')) value.pop_back();
  return value;
}

std::string phase_name(DshPhase phase) {
  switch (phase) {
    case DshPhase::Down: return "down";
    case DshPhase::Starting: return "starting";
    case DshPhase::Ready: return "ready";
    case DshPhase::Degraded: return "degraded";
    case DshPhase::Paused: return "paused";
  }
  return "unknown";
}

std::pair<std::size_t, std::size_t> context_usage(const AppState& state) {
  for (const auto& session : state.snapshot.sessions) {
    if (!state.session.empty() && session.id.value == state.session) {
      return {session.context_used, session.context_size};
    }
  }
  if (state.snapshot.sessions.size() == 1) {
    const auto& session = state.snapshot.sessions.front();
    return {session.context_used, session.context_size};
  }
  return {0, 0};
}

std::string short_path(const std::string& cwd, std::size_t keep) {
  std::vector<std::string> parts;
  for (const auto& part : std::filesystem::path(cwd)) {
    if (!part.empty() && part != "/") parts.push_back(part.string());
  }
  if (parts.empty()) return ".";
  const std::size_t first = parts.size() > keep ? parts.size() - keep : 0;
  std::string out = first == 0 ? "" : "…/";
  for (std::size_t index = first; index < parts.size(); ++index) {
    if (index != first) out += "/";
    out += parts[index];
  }
  return out;
}

}  // namespace

GitStatus read_git(const std::string& cwd) {
  GitStatus git;
  const std::string dir = shell_quote(cwd);
  git.branch = trim_line(run_command("git -C " + dir + " rev-parse --abbrev-ref HEAD 2>/dev/null"));
  if (git.branch.empty()) return git;
  git.valid = true;

  const std::string status = run_command("git -C " + dir + " status --porcelain 2>/dev/null");
  std::size_t start = 0;
  while (start < status.size()) {
    const std::size_t end = status.find('\n', start);
    const std::string line = status.substr(start, end == std::string::npos ? std::string::npos
                                                                           : end - start);
    if (line.size() >= 2) {
      if (line[0] == '?' && line[1] == '?') {
        ++git.untracked;
      } else {
        if (line[0] != ' ') ++git.staged;
        if (line[1] != ' ') ++git.modified;
      }
    }
    if (end == std::string::npos) break;
    start = end + 1;
  }
  return git;
}

RowWindow row_window(std::size_t count, std::size_t cursor, std::size_t visible) {
  RowWindow window;
  if (visible == 0) visible = 1;
  if (count == 0) return window;
  cursor = std::min(cursor, count - 1);
  window.begin = cursor >= visible ? cursor - visible + 1 : 0;
  if (window.begin + visible > count) {
    window.begin = count > visible ? count - visible : 0;
  }
  window.end = std::min(window.begin + visible, count);
  window.hidden_above = window.begin;
  window.hidden_below = count - window.end;
  return window;
}

ftxui::Element composer_view(const AppState& state, const std::string& cwd,
                             const GitStatus& git, const std::string& session_title,
                             long busy_seconds, const std::string& spinner,
                             const std::vector<std::string>& lines, std::size_t cursor_line,
                             std::size_t cursor_column, const std::string& reasoning,
                             std::size_t queued, const std::string& mode, std::size_t images,
                             MouseRouter* mouse, const std::optional<vim::Selection>& selection,
                             bool block_cursor) {
  const auto [used, size] = context_usage(state);
  const int percent = size == 0 ? 0 : static_cast<int>((used * 100) / size);
  constexpr std::size_t kVisibleRows = 8;
  const RowWindow window = row_window(lines.size(), cursor_line, kVisibleRows);
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t keep = columns >= 120 ? 3 : columns >= 110 ? 2 : 1;
  const bool compact = columns < 110;
  std::string branch = git.branch;
  const std::size_t branch_max = compact ? 10 : 16;
  branch = ellipsize(branch, branch_max);
  const ftxui::Color kBorder = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const ftxui::Color kChip = ftxui::Color::RGB(18, 18, 18);

  const auto chip = [&](std::string value, ftxui::Color fg) {
    return ftxui::text(std::move(value)) | ftxui::color(fg) | ftxui::bgcolor(kChip);
  };

  ftxui::Elements band;
  band.push_back(chip(" ", kText));
  if (busy_seconds >= 0) {
    band.push_back(chip(spinner + " " + std::to_string(busy_seconds) + "s ", kAccent));
    band.push_back(chip("", kLine));
    band.push_back(chip(" ", kText));
  }
  if (!mode.empty()) {
    band.push_back(chip(mode + " ", kAccent));
    band.push_back(chip("\xee\x82\xb1", kLine));
    band.push_back(chip(" ", kText));
  }
  if (images > 0) {
    band.push_back(chip("image " + std::to_string(images) + " ", kAccent));
    band.push_back(chip("\xee\x82\xb1", kLine));
    band.push_back(chip(" ", kText));
  }
  if (queued > 0) {
    band.push_back(chip("queue " + std::to_string(queued) + " ", kAccent));
    band.push_back(chip("\xee\x82\xb1", kLine));
    band.push_back(chip(" ", kText));
  }
  band.push_back(chip("󰵗 ", kDim));
  band.push_back(chip("", kLine));
  band.push_back(chip(" ", kText));
  band.push_back(chip("󰪟 ", kAccent));
  band.push_back(chip(state.model + " ", kText));
  if (!reasoning.empty()) {
    band.push_back(chip("\xee\x82\xb1", kLine));
    band.push_back(chip(" ", kText));
    band.push_back(chip("effort ", kDim));
    band.push_back(chip(reasoning + " ", kText));
  }
  if (!state.attachments.empty()) {
    band.push_back(chip("\xee\x82\xb1", kLine));
    band.push_back(chip(" ", kText));
    band.push_back(chip("file " + std::to_string(state.attachments.size()) + " ", kText));
  }
  band.push_back(chip("", kLine));
  band.push_back(chip(" ", kText));
  band.push_back(chip(" ", kText));
  band.push_back(chip(short_path(cwd, keep) + " ", kText));
  if (git.valid) {
    band.push_back(chip("", kLine));
    band.push_back(chip(" ", kText));
    band.push_back(chip(" ", kGit));
    band.push_back(chip(branch + " ", kGit));
    if (git.modified > 0) band.push_back(chip("*" + std::to_string(git.modified) + " ", kText));
    if (!compact && git.staged > 0) {
      band.push_back(chip("+" + std::to_string(git.staged) + " ", kGreen));
    }
    if (!compact && git.untracked > 0) {
      band.push_back(chip("?" + std::to_string(git.untracked) + " ", kGray));
    }
  }
  band.push_back(chip(" ", kText));

  ftxui::Elements top = {
      ftxui::text("╭──") | ftxui::color(kBorder),
      ftxui::hbox(std::move(band)),
      ftxui::text("") | ftxui::color(kChip),
      ftxui::text("─") | ftxui::color(kAccent),
      ftxui::text(context_bar(percent, 6)) | ftxui::color(context_color(percent)),
      ftxui::text(" "),
      ftxui::text(std::to_string(percent) + "%") | ftxui::color(context_color(percent)),
      window.hidden_above > 0
          ? (ftxui::text("↑" + std::to_string(window.hidden_above) + " ") | ftxui::color(kDim))
          : ftxui::text(""),
      ftxui::separator() | ftxui::flex | ftxui::color(kLine),
  };
  if (size > 0) {
    top.push_back(ftxui::text("󰁨") | ftxui::color(kToken));
    top.push_back(ftxui::text(compact_tokens(used) + "/" + compact_tokens(size) + " ") |
                  ftxui::color(kDim));
    top.push_back(ftxui::text("─") | ftxui::color(kBorder));
  }
  if (!session_title.empty()) {
    std::string label = session_title;
    const std::size_t max = columns > 60 ? static_cast<std::size_t>(columns) / 5 : 10;
    label = ellipsize(label, max);
    top.push_back(ftxui::text("") | ftxui::color(kChip));
    top.push_back(ftxui::text(" 󰆍 " + label + " ") | ftxui::color(kText) |
                  ftxui::bgcolor(kChip));
  }
  top.push_back(ftxui::text("─╮") | ftxui::color(kBorder));
  ftxui::Element top_line = ftxui::hbox(std::move(top));

  const std::string hint_text =
      window.hidden_below > 0
          ? "↓" + std::to_string(window.hidden_below) + " lines "
          : (state.composer.empty() ? "Enter send · ^C cancel · ^D quit " : " ");
  const ftxui::Element hint = ftxui::text(hint_text) | ftxui::dim;
  const auto render_line = [&](std::size_t row) -> ftxui::Element {
    const std::string& line = lines[row];
    const bool active = row == cursor_line;
    const std::size_t column = std::min(cursor_column, line.size());
    std::size_t sel_from = 0, sel_to = 0;
    bool selected = false;
    if (selection && row >= static_cast<std::size_t>(selection->from.line) &&
        row <= static_cast<std::size_t>(selection->to.line)) {
      selected = true;
      const bool linewise = selection->linewise;
      sel_from = linewise || row > static_cast<std::size_t>(selection->from.line)
                     ? 0
                     : static_cast<std::size_t>(selection->from.col);
      sel_to = row < static_cast<std::size_t>(selection->to.line)
                   ? line.size()
                   : static_cast<std::size_t>(selection->to.col);
      sel_from = std::min(sel_from, line.size());
      sel_to = std::min(std::max(sel_to, sel_from), line.size());
    }
    std::vector<std::size_t> cuts{0, line.size()};
    if (selected) { cuts.push_back(sel_from); cuts.push_back(sel_to); }
    if (active) {
      cuts.push_back(column);
      cuts.push_back(std::min(vim::next_grapheme_start(line, column), line.size()));
    }
    std::sort(cuts.begin(), cuts.end());
    cuts.erase(std::unique(cuts.begin(), cuts.end()), cuts.end());
    ftxui::Elements parts;
    for (std::size_t index = 0; index + 1 < cuts.size(); ++index) {
      const std::size_t from = cuts[index];
      const std::size_t to = cuts[index + 1];
      if (active && !block_cursor && from == column) parts.push_back(ftxui::text("▏") | ftxui::color(kAccent));
      auto part = ftxui::text(line.substr(from, to - from)) | ftxui::color(kText);
      const bool in_selection = selected && from >= sel_from && to <= sel_to;
      const bool on_cursor = active && block_cursor && from == column;
      if (in_selection || on_cursor) part = part | ftxui::inverted;
      parts.push_back(std::move(part));
    }
    if (active) {
      if (block_cursor && column >= line.size()) parts.push_back(ftxui::text(" ") | ftxui::inverted);
      if (!block_cursor && column >= line.size()) parts.push_back(ftxui::text("▏") | ftxui::color(kAccent));
    }
    if (selected && selection && row < static_cast<std::size_t>(selection->to.line) &&
        (selection->linewise || static_cast<std::size_t>(selection->to.line) > row)) {
      parts.push_back(ftxui::text(" ") | ftxui::inverted);
    }
    return ftxui::hbox(std::move(parts));
  };

  ftxui::Elements rows;
  rows.push_back(std::move(top_line));
  for (std::size_t row = window.begin; row < window.end; ++row) {
    ftxui::Element input_row;
    std::size_t text_column = 2;
    if (row + 1 == window.end) {
      text_column = 3;
      input_row = ftxui::hbox({
          ftxui::text("╰─ ") | ftxui::color(kBorder),
          render_line(row) | ftxui::flex,
          hint,
          ftxui::text("─╯") | ftxui::color(kBorder),
      });
    } else {
      input_row = ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(kBorder),
          render_line(row) | ftxui::flex,
          ftxui::text(" │") | ftxui::color(kBorder),
      });
    }
    if (mouse != nullptr) {
      input_row = mouse->zone(std::move(input_row), MouseSurface::Composer, row, text_column);
    }
    rows.push_back(std::move(input_row));
  }
  return ftxui::vbox(std::move(rows));
}

ftxui::Element entry_element(const TranscriptEntry& entry, bool expand_tools) {
  const ftxui::Color kChip = ftxui::Color::RGB(18, 18, 18);
  switch (entry.kind) {
    case EntryKind::Assistant:
      return ftxui::vbox({markdown_view(entry.text, false), ftxui::text(" ")});
    case EntryKind::Thought:
      return thought_view(entry);
    case EntryKind::Image: {
      std::string label = entry.text.empty() ? "image" : entry.text;
      if (!entry.tool.empty()) label += " · " + entry.tool;
      return ftxui::vbox({ftxui::text("  [image: " + label + "]") |
                              ftxui::color(kAccent) | ftxui::dim,
                          ftxui::text(" ")});
    }
    case EntryKind::Tool:
      return ftxui::vbox({tool_view(entry, expand_tools), ftxui::text(" ")});
    case EntryKind::User: {
      ftxui::Elements band;
      std::istringstream input(entry.text);
      std::string line;
      while (std::getline(input, line)) {
        band.push_back(ftxui::hbox({ftxui::text(" ") | ftxui::bgcolor(kChip),
                                    inline_words_view(line, false) | ftxui::flex |
                                        ftxui::bgcolor(kChip)}));
      }
      if (band.empty()) {
        band.push_back(ftxui::hbox({ftxui::text(" ") | ftxui::bgcolor(kChip),
                                    ftxui::text(" ") | ftxui::flex | ftxui::bgcolor(kChip)}));
      }
      band.push_back(ftxui::text(" "));
      return ftxui::vbox(std::move(band));
    }
    case EntryKind::Notice:
      return ftxui::vbox({ftxui::paragraph(entry.text) | ftxui::color(kRed), ftxui::text(" ")});
  }
  return ftxui::text(entry.text);
}

std::vector<HelpLine> help_lines() {
  return {
      {"Enter send · Alt+Enter newline · ↑/↓ history"},
      {"Tab word completion · ^G editor · ^C cancel · ^D quit"},
      {"Ctrl+Z undo · Alt+Z redo · Ctrl/Alt+←→ word"},
      {"Ctrl+W/U/K/Y kill/yank · Alt+Up edits queue"},
      {"Ctrl+R history · Ctrl+O tools · Ctrl+T thinking"},
      {"Alt+H help · Alt+M model · Alt+S sessions · Alt+C copy"},
      {"Ctrl+V image · :tada: emoji · BOOTH_VIM for Vim"},
      {"!command runs local /bin/sh · output stays local"},
      {"$python runs Python locally; an expression prints its value"},
      {"^model mention applies a model · Alt+M picks one"},
      {"/ fuzzy commands · ↑/↓ wheel · Tab · Enter · Esc"},
      {"PageUp/PageDown or wheel browse transcript"},
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
      {"/copy last answer · /clear clear transcript"},
      {"/retry last prompt · /export markdown transcript"},
      {"/sessions resume · /new fresh · /close stop"},
      {"/session /acp /cancel · /find · /attach <path> · /detach"},
      {"/help this card · /quit leave"},
      {"Aliases: /resume /models /reasoning /hotkeys /keys /exit /q", true},
  };
}

int help_visible_lines() {
  const int rows = std::max(12, ftxui::Terminal::Size().dimy);
  return std::max(4, rows - 7);
}

int help_max_offset() {
  const int total = static_cast<int>(help_lines().size());
  return std::max(0, total - std::min(total, help_visible_lines()));
}

ftxui::Element help_view(int offset, MouseRouter* mouse) {
  const auto lines = help_lines();
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

void collect_model_options(const nlohmann::json& option, std::vector<ModelChoice>& out,
                           const std::string& provider) {
  if (!option.is_object()) return;
  if (option.contains("value") && option["value"].is_string()) {
    out.push_back(ModelChoice{provider, option.value("name", std::string{}),
                              option["value"].get<std::string>()});
  }
  if (option.contains("options")) {
    std::string next = provider;
    if (option.contains("name") && option["name"].is_string()) {
      next = option["name"].get<std::string>();
    } else if (option.contains("group") && option["group"].is_string()) {
      next = option["group"].get<std::string>();
    }
    for (const auto& child : option["options"]) {
      collect_model_options(child, out, next);
    }
  }
}

std::string read_model_options(const nlohmann::json& config_options,
                               std::vector<ModelChoice>& choices) {
  choices.clear();
  for (const auto& option : config_options) {
    if (!option.is_object() || option.value("id", std::string{}) != "model") continue;
    const std::string current = option.value("currentValue", std::string{});
    for (const auto& group : option.value("options", nlohmann::json::array())) {
      collect_model_options(group, choices);
    }
    for (const auto& choice : choices) {
      if (choice.value == current) return choice.name;
    }
    const auto parsed = nlohmann::json::parse(current, nullptr, false);
    if (parsed.is_array() && !parsed.empty() && parsed.back().is_string()) {
      return parsed.back().get<std::string>();
    }
    return current;
  }
  return {};
}

ReasoningOptions read_reasoning_options(const nlohmann::json& config_options) {
  ReasoningOptions options;
  for (const auto& option : config_options) {
    if (!option.is_object() || option.value("category", std::string{}) != "thought_level") {
      continue;
    }
    options.id = option.value("id", std::string{});
    options.current = option.value("currentValue", std::string{});
    for (const auto& group : option.value("options", nlohmann::json::array())) {
      collect_model_options(group, options.choices);
    }
    for (const auto& choice : options.choices) {
      if (choice.value == options.current) {
        options.current_name = choice.name;
        break;
      }
    }
    if (options.current_name.empty()) options.current_name = options.current;
    break;
  }
  return options;
}

std::size_t display_width(const std::string& value) {
  std::size_t width = 0;
  for (const char ch : value) {
    if ((static_cast<unsigned char>(ch) & 0xC0) != 0x80) ++width;
  }
  return width;
}

void print_splash() {
  if (::isatty(STDOUT_FILENO) == 0) return;
  const std::vector<std::string> logo = {
      " ██████╗  ██████╗  ██████╗ ████████╗██╗  ██╗",
      " ██╔══██╗██╔═══██╗██╔═══██╗╚══██╔══╝██║  ██║",
      " ██████╔╝██║   ██║██║   ██║   ██║   ███████║",
      " ██╔══██╗██║   ██║██║   ██║   ██║   ██╔══██║",
      " ██████╔╝╚██████╔╝╚██████╔╝   ██║   ██║  ██║",
      " ╚═════╝  ╚═════╝  ╚═════╝    ╚═╝   ╚═╝  ╚═╝",
  };
  winsize window{};
  const int columns = ::ioctl(STDOUT_FILENO, TIOCGWINSZ, &window) == 0 && window.ws_col > 0
                          ? static_cast<int>(window.ws_col)
                          : 80;
  const auto center = [&](std::size_t width) {
    const int pad = std::max(0, (columns - static_cast<int>(width)) / 2);
    return std::string(static_cast<std::size_t>(pad), ' ');
  };
  const char* shades[] = {"\x1b[38;2;0;175;255m", "\x1b[38;2;0;175;255m",
                          "\x1b[38;2;95;135;175m", "\x1b[38;2;95;135;175m",
                          "\x1b[38;2;118;118;118m", "\x1b[38;2;72;72;72m"};
  std::cout << '\n';
  for (std::size_t index = 0; index < logo.size(); ++index) {
    std::cout << center(display_width(logo[index])) << shades[index] << logo[index]
              << "\x1b[0m\n";
  }
  std::cout << center(30) << "\x1b[2mbooth · dsh console · v" << BOOTH_VERSION
            << "\x1b[0m\n";
  std::cout << center(60) << "\x1b[2mTip: /help lists keys and commands.\x1b[0m\n\n";
  std::cout.flush();
}

std::string status_text(const AppState& state) {
  std::string out = "booth · " + phase_name(state.snapshot.phase);
  if (!state.session.empty()) out += " · session " + state.session;
  out += " · model " + state.model;
  const auto [used, size] = context_usage(state);
  if (size > 0) out += " · context " + std::to_string(used * 100 / size) + "%";
  return out;
}

}  // namespace booth
