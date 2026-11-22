#include "interaction/view.hpp"

#include <ftxui/screen/terminal.hpp>
#include <sys/ioctl.h>
#include <unistd.h>

#include <iostream>
#include <nlohmann/json.hpp>

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);
const ftxui::Color kLine = ftxui::Color::RGB(72, 72, 72);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);
const ftxui::Color kYellow = ftxui::Color::RGB(255, 215, 0);
const ftxui::Color kRed = ftxui::Color::RGB(255, 95, 95);
const ftxui::Color kPanel = ftxui::Color::RGB(18, 18, 18);
const ftxui::Color kGit = ftxui::Color::RGB(255, 175, 95);
const ftxui::Color kGray = ftxui::Color::RGB(168, 168, 168);
const ftxui::Color kToken = ftxui::Color::RGB(95, 135, 175);

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

ftxui::Element composer_view(const AppState& state, const std::string& cwd,
                             const GitStatus& git, const std::string& session_title,
                             long busy_seconds, const std::string& spinner,
                             const std::vector<std::string>& lines, std::size_t cursor_line,
                             std::size_t cursor_column, const std::string& reasoning) {
  const auto [used, size] = context_usage(state);
  const int percent = size == 0 ? 0 : static_cast<int>((used * 100) / size);
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t keep = columns >= 120 ? 3 : columns >= 110 ? 2 : 1;
  const bool compact = columns < 110;
  std::string branch = git.branch;
  const std::size_t branch_max = compact ? 10 : 16;
  if (branch.size() > branch_max) branch = branch.substr(0, branch_max - 1) + "…";
  const ftxui::Color kBorder = ftxui::Color::RGB(108, 108, 108);
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
      ftxui::text(std::to_string(percent) + "%") | ftxui::color(kDim),
      ftxui::separator() | ftxui::flex | ftxui::color(kLine),
  };
  if (size > 0) {
    top.push_back(ftxui::text("󰁨") | ftxui::color(kToken));
    top.push_back(ftxui::text(compact_tokens(size) + " ") | ftxui::color(kDim));
    top.push_back(ftxui::text("─") | ftxui::color(kBorder));
  }
  if (!session_title.empty()) {
    std::string label = session_title;
    const std::size_t max = columns > 60 ? static_cast<std::size_t>(columns) / 5 : 10;
    if (label.size() > max) label = label.substr(0, max - 1) + "…";
    top.push_back(ftxui::text("") | ftxui::color(kChip));
    top.push_back(ftxui::text(" 󰆍 " + label + " ") | ftxui::color(kText) |
                  ftxui::bgcolor(kChip));
  }
  top.push_back(ftxui::text("─╮") | ftxui::color(kBorder));
  ftxui::Element top_line = ftxui::hbox(std::move(top));

  const ftxui::Element hint = state.composer.empty()
                                  ? ftxui::text("Enter send · ^C cancel · ^D quit ") | ftxui::dim
                                  : ftxui::text(" ");
  const auto render_line = [&](std::size_t row) -> ftxui::Element {
    const std::string& line = lines[row];
    if (row != cursor_line) return ftxui::text(line) | ftxui::color(kText);
    const std::size_t column = std::min(cursor_column, line.size());
    return ftxui::hbox({
        ftxui::text(line.substr(0, column)) | ftxui::color(kText),
        ftxui::text("▏") | ftxui::color(kAccent),
        ftxui::text(line.substr(column)) | ftxui::color(kText),
    });
  };

  ftxui::Elements rows;
  rows.push_back(std::move(top_line));
  if (lines.size() <= 1) {
    rows.push_back(ftxui::hbox({
        ftxui::text("╰─ ") | ftxui::color(kBorder),
        render_line(0) | ftxui::flex,
        hint,
        ftxui::text("─╯") | ftxui::color(kBorder),
    }));
  } else {
    for (std::size_t row = 0; row + 1 < lines.size(); ++row) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(kBorder),
          render_line(row) | ftxui::flex,
          ftxui::text(" │") | ftxui::color(kBorder),
      }));
    }
    rows.push_back(ftxui::hbox({
        ftxui::text("╰─ ") | ftxui::color(kBorder),
        render_line(lines.size() - 1) | ftxui::flex,
        hint,
        ftxui::text("─╯") | ftxui::color(kBorder),
    }));
  }
  return ftxui::vbox(std::move(rows));
}

std::vector<std::string> wrap_cells(const std::string& text, std::size_t width) {
  std::vector<std::string> out;
  if (width == 0) width = 1;
  for (const auto& logical : [&] {
         std::vector<std::string> lines;
         std::istringstream input(text);
         std::string line;
         while (std::getline(input, line)) lines.push_back(line);
         if (lines.empty()) lines.push_back("");
         return lines;
       }()) {
    std::string row;
    std::size_t cells = 0;
    for (const char ch : logical) {
      const bool continuation = (static_cast<unsigned char>(ch) & 0xC0) == 0x80;
      if (!continuation && cells == width) {
        out.push_back(row);
        row.clear();
        cells = 0;
      }
      row += ch;
      if (!continuation) ++cells;
    }
    out.push_back(row);
  }
  return out;
}

ftxui::Element tool_view(const TranscriptEntry& entry) {
  const ftxui::Color border = ftxui::Color::RGB(108, 108, 108);
  std::string title = entry.text.empty() ? entry.tool : entry.text;
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t content_width =
      columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;
  if (title.size() > content_width) title = title.substr(0, content_width - 1) + "…";
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text("╭─── ") | ftxui::color(border),
      ftxui::text(title) | ftxui::bold |
          ftxui::color(entry.status == "failed" ? kRed : kText),
      ftxui::text(" "),
      ftxui::separator() | ftxui::flex | ftxui::color(border),
      ftxui::text("╮") | ftxui::color(border),
  }));
  if (!entry.input.empty()) {
    for (const auto& line : wrap_cells(entry.input, content_width)) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│   ") | ftxui::color(border),
          ftxui::text(line) | ftxui::flex | ftxui::color(kText),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (!entry.output.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("├── Output ") | ftxui::color(border),
        ftxui::separator() | ftxui::flex | ftxui::color(border),
        ftxui::text("┤") | ftxui::color(border),
    }));
    std::istringstream output(entry.output);
    std::string line;
    std::size_t shown = 0;
    std::size_t hidden = 0;
    while (std::getline(output, line)) {
      if (++shown > 12) {
        ++hidden;
        continue;
      }
      for (const auto& wrapped : wrap_cells(line, content_width)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(kText),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    }
    if (hidden > 0) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(hidden) + " lines") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  std::string footer = entry.status.empty() ? "running" : entry.status;
  if (entry.finished > entry.started && entry.started != std::chrono::system_clock::time_point{}) {
    const auto ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(entry.finished - entry.started);
    footer = std::to_string(ms.count()) + "ms";
  }
  rows.push_back(ftxui::hbox({
      ftxui::text("╰─── ") | ftxui::color(border),
      ftxui::text(footer) | ftxui::dim,
      ftxui::text(" ") ,
      ftxui::separator() | ftxui::flex | ftxui::color(border),
      ftxui::text("╯") | ftxui::color(border),
  }));
  return ftxui::vbox(std::move(rows));
}

ftxui::Element entry_element(const TranscriptEntry& entry) {
  const ftxui::Color kChip = ftxui::Color::RGB(18, 18, 18);
  switch (entry.kind) {
    case EntryKind::Assistant:
      return ftxui::vbox({markdown_view(entry.text, false), ftxui::text(" ")});
    case EntryKind::Thought:
      return ftxui::vbox({markdown_view(entry.text, true), ftxui::text(" ")});
    case EntryKind::Tool:
      return ftxui::vbox({tool_view(entry), ftxui::text(" ")});
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

ftxui::Element help_view() {
  ftxui::Elements body;
  body.push_back(ftxui::text("booth — keys and commands") | ftxui::bold | ftxui::color(kAccent));
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  body.push_back(ftxui::text("Enter send · ↑/↓ history · Tab complete · ^G editor · ^C cancel · ^D quit"));
  body.push_back(ftxui::text("PageUp/PageDown or wheel  browse the transcript"));
  body.push_back(ftxui::text("/copy /clear /export /status /help /quit work during a run; Enter queues"));
  body.push_back(ftxui::text("/status  show phase, session and model"));
  body.push_back(ftxui::text("/model   auto|<id>"));
  body.push_back(ftxui::text("/copy    copy the last answer"));
  body.push_back(ftxui::text("/clear   clear the local transcript"));
  body.push_back(ftxui::text("/retry   send the last prompt again"));
  body.push_back(ftxui::text("/export  write the transcript as markdown"));
  body.push_back(ftxui::text("/sessions  resume a saved session"));
  body.push_back(ftxui::text("/close   close the active session"));
  body.push_back(ftxui::text("/help    this card · /quit leave"));
  body.push_back(ftxui::text("Shift+Tab  cycle reasoning effort"));
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  body.push_back(ftxui::text("Esc or q closes") | ftxui::dim);
  return ftxui::hbox({ftxui::text(" "), ftxui::vbox(std::move(body)), ftxui::filler()}) |
         ftxui::bgcolor(kPanel);
}

ftxui::Element permission_view(const PermissionRequest& request) {
  ftxui::Elements body;
  body.push_back(ftxui::text("permission: " + request.tool) | ftxui::bold | ftxui::color(kAccent));
  if (!request.detail.empty()) {
    const auto parsed = nlohmann::json::parse(request.detail, nullptr, false);
    if (!parsed.is_discarded() && parsed.contains("command") && parsed["command"].is_string()) {
      body.push_back(ftxui::paragraph(parsed["command"].get<std::string>()));
    } else {
      body.push_back(ftxui::paragraph(request.detail));
    }
  }
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  body.push_back(ftxui::hbox({
      ftxui::text("[y] Allow once") | ftxui::color(kGreen),
      ftxui::text("    [a] Allow always") | ftxui::color(kGreen),
  }));
  body.push_back(ftxui::hbox({
      ftxui::text("[n] Deny") | ftxui::color(kRed),
      ftxui::text("          [d] Deny always") | ftxui::color(kRed),
      ftxui::text("    (Esc)") | ftxui::dim,
  }));
  return ftxui::hbox({ftxui::text(" "), ftxui::vbox(std::move(body)), ftxui::filler()}) |
         ftxui::bgcolor(kPanel);
}

constexpr CommandHint kCommandHints[] = {
    {"/help", "keys and commands"},
    {"/model", "switch the session model"},
    {"/status", "phase, session and model"},
    {"/copy", "copy the last answer"},
    {"/export", "write the transcript as markdown"},
    {"/sessions", "resume a saved session"},
    {"/close", "close the active session"},
    {"/quit", "leave"},
};

std::vector<const CommandHint*> matching_commands(const std::string& composer) {
  std::vector<const CommandHint*> matches;
  if (composer.empty() || composer.front() != '/') return matches;
  if (composer.find(' ') != std::string::npos) return matches;
  for (const auto& hint : kCommandHints) {
    if (std::string(hint.name).rfind(composer, 0) == 0) matches.push_back(&hint);
  }
  return matches;
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
  return out;
}

}  // namespace booth
