#include "interaction/trace_report.hpp"

#include <chrono>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string_view>
#include <system_error>

namespace booth {
namespace {

std::string escape(std::string_view text) {
  std::string out;
  out.reserve(text.size());
  for (const char ch : text) {
    switch (ch) {
      case '&': out += "&amp;"; break;
      case '<': out += "&lt;"; break;
      case '>': out += "&gt;"; break;
      case '"': out += "&quot;"; break;
      case '\'': out += "&#39;"; break;
      default: out += ch; break;
    }
  }
  return out;
}

std::string kind_name(EntryKind kind) {
  switch (kind) {
    case EntryKind::User: return "user";
    case EntryKind::Assistant: return "assistant";
    case EntryKind::Thought: return "thought";
    case EntryKind::Tool: return "tool";
    case EntryKind::Notice: return "notice";
    case EntryKind::Command: return "command";
    case EntryKind::Image: return "image";
  }
  return "unknown";
}

std::string clock_time(std::chrono::system_clock::time_point point) {
  if (point.time_since_epoch().count() == 0) return "-";
  const std::time_t seconds = std::chrono::system_clock::to_time_t(point);
  std::tm local{};
  localtime_r(&seconds, &local);
  char text[16];
  std::strftime(text, sizeof(text), "%H:%M:%S", &local);
  return text;
}

std::string duration(std::chrono::system_clock::time_point started,
                     std::chrono::system_clock::time_point finished) {
  if (started.time_since_epoch().count() == 0 || finished < started) return "-";
  const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(finished - started).count();
  return std::to_string(ms) + " ms";
}

}  // namespace

std::string render_trace_report(const AppState& state) {
  std::size_t tools = 0;
  std::size_t failures = 0;
  for (const auto& entry : state.entries) {
    if (entry.kind == EntryKind::Tool) ++tools;
    if (!entry.status.empty() && entry.status != "running" && entry.status != "complete" &&
        entry.status != "ok" && entry.status != "completed") {
      ++failures;
    }
  }
  std::ostringstream html;
  html << "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">"
       << "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">"
       << "<title>booth trace</title><style>"
       << "body{font:14px/1.45 system-ui,sans-serif;margin:2rem;max-width:72rem;color:#ddd;background:#161616}"
       << "h1{font-size:1.3rem}h2{font-size:1rem;margin-top:2rem;color:#aaa}"
       << ".meta{color:#aaa}table{border-collapse:collapse;width:100%}"
       << "th,td{border-bottom:1px solid #333;padding:.4rem;text-align:left;vertical-align:top}"
       << "th{color:#aaa;font-weight:500}pre{white-space:pre-wrap;margin:0;font:12px/1.35 ui-monospace,monospace}"
       << ".tool{color:#8bd5ff}.error{color:#ff8a80}.muted{color:#777}"
       << "</style><h1>booth session trace</h1>"
       << "<p class=\"meta\">session " << escape(state.session.empty() ? "(none)" : state.session)
       << " · model " << escape(state.model) << " · " << state.entries.size() << " entries · "
       << tools << " tools · " << failures << " non-complete</p>";
  html << "<h2>Transcript</h2><table><thead><tr><th>time</th><th>kind</th><th>tool</th>"
       << "<th>status</th><th>duration</th><th>content</th></tr></thead><tbody>";
  for (const auto& entry : state.entries) {
    html << "<tr><td class=\"muted\">" << clock_time(entry.started) << "</td><td>"
         << kind_name(entry.kind) << "</td><td class=\"tool\">" << escape(entry.tool)
         << "</td><td class=\"" << (entry.status == "error" ? "error" : "muted") << "\">"
         << escape(entry.status) << "</td><td class=\"muted\">" << duration(entry.started, entry.finished)
         << "</td><td><pre>" << escape(entry.text) << "</pre>";
    if (!entry.input.empty()) html << "<pre class=\"muted\">" << escape(entry.input) << "</pre>";
    if (!entry.output.empty()) html << "<pre>" << escape(entry.output) << "</pre>";
    html << "</td></tr>";
  }
  html << "</tbody></table></html>";
  return html.str();
}

bool write_trace_report(const AppState& state, const std::filesystem::path& path, std::string& error) {
  std::error_code ec;
  if (!path.parent_path().empty()) std::filesystem::create_directories(path.parent_path(), ec);
  if (ec) {
    error = ec.message();
    return false;
  }
  std::ofstream file(path, std::ios::out | std::ios::trunc | std::ios::binary);
  if (!file.is_open()) {
    error = "cannot open " + path.string();
    return false;
  }
  file << render_trace_report(state);
  file.close();
  std::filesystem::permissions(path, std::filesystem::perms::owner_read | std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, ec);
  return true;
}

}  // namespace booth
