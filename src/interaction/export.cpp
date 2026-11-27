#include "interaction/export.hpp"

#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <ctime>
#include <fstream>
#include <string>
#include <string_view>

namespace booth {
namespace {

std::string stamp() {
  const std::time_t now = std::time(nullptr);
  std::tm local{};
  localtime_r(&now, &local);
  char buffer[32];
  std::strftime(buffer, sizeof(buffer), "%Y%m%d-%H%M%S", &local);
  return buffer;
}

std::string duration(const TranscriptEntry& entry) {
  if (entry.finished <= entry.started ||
      entry.started == std::chrono::system_clock::time_point{}) {
    return {};
  }
  const auto ms =
      std::chrono::duration_cast<std::chrono::milliseconds>(entry.finished - entry.started);
  return std::to_string(ms.count()) + "ms";
}

std::string escape_html(std::string_view text, bool pre = false) {
  std::string out;
  out.reserve(text.size());
  for (const char ch : text) {
    switch (ch) {
      case '&': out += "&amp;"; break;
      case '<': out += "&lt;"; break;
      case '>': out += "&gt;"; break;
      case '"': out += "&quot;"; break;
      case '\n': out += pre ? "\n" : "<br>"; break;
      default: out += ch;
    }
  }
  return out;
}

std::string lower_extension(const std::filesystem::path& path) {
  std::string ext = path.extension().string();
  for (char& ch : ext) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return ext;
}

}  // namespace

std::string transcript_markdown(const AppState& state) {
  std::string out = "# booth transcript\n\n";
  if (!state.session.empty()) out += "- session: " + state.session + "\n";
  if (!state.title.empty()) out += "- title: " + state.title + "\n";
  if (!state.model.empty()) out += "- model: " + state.model + "\n";
  if (!state.session.empty() || !state.title.empty() || !state.model.empty()) out += "\n";
  for (const auto& entry : state.entries) {
    switch (entry.kind) {
      case EntryKind::User:
        out += "## You\n\n" + entry.text + "\n\n";
        break;
      case EntryKind::Assistant:
        out += "## Assistant\n\n" + entry.text + "\n\n";
        break;
      case EntryKind::Thought:
        out += "> " + entry.text + "\n\n";
        break;
      case EntryKind::Image:
        out += "> **image**: " + entry.text + (entry.tool.empty() ? "" : " · " + entry.tool) + "\n\n";
        break;
      case EntryKind::Notice:
        out += "> **notice**: " + entry.text + "\n\n";
        break;
      case EntryKind::Command:
        out += "### Command: " + (entry.tool.empty() ? "command" : entry.tool) + "\n\n" +
               entry.text + "\n\n";
        break;
      case EntryKind::Tool: {
        out += "### Tool: " + (entry.tool.empty() ? entry.text : entry.tool) + "\n\n";
        if (!entry.status.empty()) out += "- status: " + entry.status + "\n";
        const std::string took = duration(entry);
        if (!took.empty()) out += "- duration: " + took + "\n";
        if (!entry.input.empty()) out += "\nInput:\n\n```\n" + entry.input + "\n```\n";
        const bool read = entry.tool == "read" || entry.tool == "read_file";
        const std::string viewed = read ? read_output_text(entry.output) : entry.output;
        if (!viewed.empty()) out += "\nOutput:\n\n```\n" + viewed + "\n```\n";
        out += "\n";
        break;
      }
    }
  }
  return out;
}

std::string transcript_html(const AppState& state) {
  const std::string title = state.title.empty() ? "booth transcript" : state.title;
  std::string out =
      "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">"
      "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
      "<title>" + escape_html(title) + "</title><style>"
      ":root{color-scheme:light dark}body{margin:0 auto;max-width:60rem;padding:2rem;"
      "font:16px/1.55 ui-sans-serif,system-ui,sans-serif;background:Canvas;color:CanvasText}"
      "header{border-bottom:1px solid color-mix(in srgb,CanvasText 20%,transparent);margin-bottom:2rem}"
      "dl{display:grid;grid-template-columns:max-content 1fr;gap:.25rem 1rem;font-size:.9rem}"
      "dt{opacity:.65}section{margin:2rem 0}blockquote{border-left:3px solid color-mix(in srgb,"
      "CanvasText 25%,transparent);margin:0;padding:.25rem 1rem;opacity:.8}pre{white-space:pre-wrap;"
      "overflow-wrap:anywhere;background:color-mix(in srgb,CanvasText 7%,transparent);padding:1rem;"
      "border-radius:.5rem}.tool{border:1px solid color-mix(in srgb,CanvasText 18%,transparent);"
      "border-radius:.5rem;padding:1rem}.muted{opacity:.65}</style></head><body>"
      "<header><h1>" + escape_html(title) + "</h1><dl>";
  if (!state.session.empty()) out += "<dt>session</dt><dd>" + escape_html(state.session) + "</dd>";
  if (!state.model.empty()) out += "<dt>model</dt><dd>" + escape_html(state.model) + "</dd>";
  out += "</dl></header><main>";
  for (const auto& entry : state.entries) {
    const std::string text = escape_html(entry.text, true);
    switch (entry.kind) {
      case EntryKind::User: out += "<section class=\"user\"><h2>You</h2><pre>" + text + "</pre></section>"; break;
      case EntryKind::Assistant: out += "<section class=\"assistant\"><h2>Assistant</h2><pre>" + text + "</pre></section>"; break;
      case EntryKind::Thought: out += "<blockquote><pre>" + text + "</pre></blockquote>"; break;
      case EntryKind::Image: out += "<p class=\"muted\"><strong>image</strong>: " + text + (entry.tool.empty() ? "" : " · " + escape_html(entry.tool)) + "</p>"; break;
      case EntryKind::Notice: out += "<p class=\"muted\"><strong>notice</strong>: " + text + "</p>"; break;
      case EntryKind::Command: out += "<section><h2>Command: " + escape_html(entry.tool.empty() ? "command" : entry.tool) + "</h2><pre>" + text + "</pre></section>"; break;
      case EntryKind::Tool: {
        out += "<section class=\"tool\"><h3>Tool: " + escape_html(entry.tool.empty() ? entry.text : entry.tool) + "</h3>";
        if (!entry.status.empty()) out += "<p class=\"muted\">status: " + escape_html(entry.status) + "</p>";
        const std::string took = duration(entry);
        if (!took.empty()) out += "<p class=\"muted\">duration: " + took + "</p>";
        if (!entry.input.empty()) out += "<h4>Input</h4><pre>" + escape_html(entry.input, true) + "</pre>";
        const bool read = entry.tool == "read" || entry.tool == "read_file";
        const std::string viewed = read ? read_output_text(entry.output) : entry.output;
        if (!viewed.empty()) out += "<h4>Output</h4><pre>" + escape_html(viewed, true) + "</pre>";
        out += "</section>";
        break;
      }
    }
  }
  out += "</main></body></html>\n";
  return out;
}

void export_command(AppState& state, const std::string& line,
                    const std::filesystem::path& state_dir, const std::string& cwd) {
  std::string args = line.size() > 7 ? line.substr(8) : std::string{};
  while (!args.empty() && (args.front() == ' ' || args.front() == '\t')) args.erase(args.begin());
  while (!args.empty() && (args.back() == ' ' || args.back() == '\t')) args.pop_back();
  bool themes = false;
  if (args == "--themes" || args.rfind("--themes ", 0) == 0) {
    themes = true;
    args = args.size() > 8 ? args.substr(9) : std::string{};
    while (!args.empty() && (args.front() == ' ' || args.front() == '\t')) args.erase(args.begin());
  }
  if (args == "--copy" || args == "clipboard" || args == "copy") {
    state.notice = "use /dump to copy the session to the clipboard";
    return;
  }
  if (!args.empty() && args[0] == '-') {
    state.notice = "usage: /export [--themes] [path]";
    return;
  }
  std::filesystem::path target;
  if (!args.empty()) {
    target = args;
    if (target.is_relative()) target = std::filesystem::path(cwd) / target;
  }
  bool html = themes;
  if (target.empty()) {
    target = state_dir / ("transcript-" + stamp() + ".html");
    html = true;
  } else {
    const std::string ext = lower_extension(target);
    if (ext.empty()) { target += ".html"; html = true; }
    else html = themes || (ext != ".md" && ext != ".markdown");
  }

  std::error_code error;
  if (target.has_parent_path()) {
    std::filesystem::create_directories(target.parent_path(), error);
    if (error) {
      state.notice = "cannot create " + target.parent_path().string();
      return;
    }
  }
  auto temp = target;
  temp += ".tmp";
  std::ofstream output(temp, std::ios::trunc);
  if (!output) {
    state.notice = "cannot write " + target.string();
    return;
  }
  output << (html ? transcript_html(state) : transcript_markdown(state));
  output.close();
  if (!output) {
    state.notice = "cannot write " + target.string();
    return;
  }
  std::filesystem::rename(temp, target, error);
  if (error) {
    state.notice = "cannot replace " + target.string();
    return;
  }
  std::filesystem::permissions(target,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
  state.notice = "exported " + std::to_string(state.entries.size()) + " entries to " +
                 target.string();
}

}  // namespace booth
