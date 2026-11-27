#include "interaction/extra_commands.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <cstdlib>
#include <ctime>
#include <fstream>
#include <iterator>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include <nlohmann/json.hpp>

#include "interaction/approvals.hpp"
#include "interaction/changelog.hpp"
#include "interaction/clipboard.hpp"
#include "interaction/export.hpp"
#include "interaction/link_picker.hpp"
#include "interaction/loop_controller.hpp"
#include "interaction/mcp_config.hpp"
#include "interaction/open_link.hpp"
#include "interaction/recorder.hpp"
#include "interaction/restart.hpp"
#include "interaction/ssh_config.hpp"
#include "interaction/trace_report.hpp"

namespace booth {
namespace {

std::vector<std::string> split_words(std::string_view text) {
  std::vector<std::string> words;
  std::string current;
  char quote = 0;
  for (const char ch : text) {
    if (quote != 0) {
      if (ch == quote) quote = 0;
      else current += ch;
      continue;
    }
    if (ch == '\'' || ch == '"') {
      quote = ch;
      continue;
    }
    if (ch == ' ' || ch == '\t') {
      if (!current.empty()) {
        words.push_back(std::move(current));
        current.clear();
      }
      continue;
    }
    current += ch;
  }
  if (!current.empty()) words.push_back(std::move(current));
  return words;
}

std::string trim(std::string text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return text.substr(first, last - first + 1);
}

std::string kind_name(EntryKind kind) {
  switch (kind) {
    case EntryKind::User: return "user";
    case EntryKind::Assistant: return "assistant";
    case EntryKind::Thought: return "thought";
    case EntryKind::Tool: return "tool";
    case EntryKind::Notice: return "notice";
    case EntryKind::Image: return "image";
    case EntryKind::Command: return "command";
  }
  return "unknown";
}

nlohmann::json dump_json(const AppState& state, bool anonymized) {
  nlohmann::json entries = nlohmann::json::array();
  for (const auto& entry : state.entries) {
    nlohmann::json row = {{"kind", kind_name(entry.kind)},
                          {"text", anonymized ? "<redacted>" : entry.text}};
    if (!entry.tool.empty()) row["tool"] = entry.tool;
    if (!entry.status.empty()) row["status"] = entry.status;
    if (!anonymized && !entry.input.empty()) row["input"] = entry.input;
    if (!anonymized && !entry.output.empty()) row["output"] = entry.output;
    entries.push_back(std::move(row));
  }
  return {{"session", anonymized ? "<redacted>" : state.session},
          {"title", anonymized ? "<redacted>" : state.title},
          {"model", state.model},
          {"anonymized", anonymized},
          {"entries", std::move(entries)}};
}

std::string env_or(const char* name, const std::string& fallback) {
  const char* value = ::getenv(name);
  return value == nullptr || *value == '\0' ? fallback : std::string(value);
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

std::string debug_text(const AppState& state) {
  std::size_t tools = 0;
  std::size_t thoughts = 0;
  for (const auto& entry : state.entries) {
    if (entry.kind == EntryKind::Tool) ++tools;
    if (entry.kind == EntryKind::Thought) ++thoughts;
  }
  std::string out = "phase " + phase_name(state.snapshot.phase);
  out += " · session " + (state.session.empty() ? std::string("none") : state.session);
  out += " · model " + state.model;
  if (!state.reasoning.current_name.empty()) out += " · effort " + state.reasoning.current_name;
  out += " · entries " + std::to_string(state.entries.size()) + " · tools " + std::to_string(tools) +
         " · thoughts " + std::to_string(thoughts) + " · todos " + std::to_string(state.todos.size());
  if (state.busy) out += " · busy";
  if (state.permission) out += " · permission";
  if (state.loop_enabled) out += " · loop";
  if (state.restart_requested) out += " · restart";
  return out;
}

std::string extensions_text() {
  const std::string home = env_or("DSH_HOME", env_or("HOME", ".") + "/.dsh");
  const std::string base = env_or("BOOTH_BASE_PROFILE", env_or("BOOTH_PROFILE", "booth"));
  const auto dir = std::filesystem::path(home) / "profiles" / base;
  const auto package = dir / "package.json";
  const auto patch = dir / "cordis.patch.yml";
  std::ifstream package_input(package);
  if (!package_input) return "no dsh profile at " + dir.string();
  const auto document = nlohmann::json::parse(package_input, nullptr, false);
  if (document.is_discarded() || !document.is_object()) return "unreadable " + package.string();
  std::string bundles;
  std::size_t bundle_count = 0;
  if (document.contains("dsh") && document["dsh"].is_object() && document["dsh"].contains("profile") &&
      document["dsh"]["profile"].is_object() && document["dsh"]["profile"].contains("bundles") &&
      document["dsh"]["profile"]["bundles"].is_array()) {
    for (const auto& bundle : document["dsh"]["profile"]["bundles"]) {
      if (!bundle.is_string()) continue;
      if (!bundles.empty()) bundles += ", ";
      bundles += bundle.get<std::string>();
      ++bundle_count;
    }
  }
  std::size_t dependencies = 0;
  if (document.contains("dependencies") && document["dependencies"].is_object()) {
    dependencies = document["dependencies"].size();
  }
  std::size_t rows = 0;
  std::ifstream patch_input(patch);
  std::string line;
  while (std::getline(patch_input, line)) {
    const auto first = line.find_first_not_of(" \t");
    if (first != std::string::npos && line.compare(first, 4, "- id") == 0) ++rows;
  }
  std::string out = "extensions " + base + " · bundles " + std::to_string(bundle_count);
  if (!bundles.empty()) out += " (" + bundles + ")";
  out += " · dependencies " + std::to_string(dependencies);
  out += " · patch rows " + std::to_string(rows);
  return out;
}

std::string plugins_text(std::string_view args) {
  std::string verb = "list";
  const auto words = split_words(args);
  if (!words.empty()) verb = words.front();
  if (verb == "enable" || verb == "disable") {
    return "dsh profile plugins are managed by package.json and cordis.patch.yml; edit the profile "
           "and run /reload-plugins";
  }
  if (verb != "list") return "Usage: /plugins [list|enable|disable] <plugin>";
  const std::string home = env_or("DSH_HOME", env_or("HOME", ".") + "/.dsh");
  const std::string base = env_or("BOOTH_BASE_PROFILE", env_or("BOOTH_PROFILE", "booth"));
  const auto dir = std::filesystem::path(home) / "profiles" / base;
  const auto package = dir / "package.json";
  std::ifstream input(package);
  if (!input) return "no dsh profile at " + dir.string();
  const auto document = nlohmann::json::parse(input, nullptr, false);
  if (document.is_discarded() || !document.is_object()) return "unreadable " + package.string();
  std::string out = "plugins " + dir.string();
  out += "\ndependencies:";
  bool any = false;
  if (document.contains("dependencies") && document["dependencies"].is_object()) {
    for (auto it = document["dependencies"].begin(); it != document["dependencies"].end(); ++it) {
      out += "\n  " + it.key() + " " + (it.value().is_string() ? it.value().get<std::string>() : "?");
      any = true;
    }
  }
  if (!any) out += "\n  (none)";
  out += "\nbundles:";
  any = false;
  if (document.contains("dsh") && document["dsh"].is_object() && document["dsh"].contains("profile") &&
      document["dsh"]["profile"].is_object() && document["dsh"]["profile"].contains("bundles") &&
      document["dsh"]["profile"]["bundles"].is_array()) {
    for (const auto& bundle : document["dsh"]["profile"]["bundles"]) {
      if (!bundle.is_string()) continue;
      out += " " + bundle.get<std::string>();
      any = true;
    }
  }
  if (!any) out += " (none)";
  return out;
}

std::string stamp_now() {
  const auto now = std::chrono::system_clock::now();
  const auto seconds = std::chrono::system_clock::to_time_t(now);
  std::tm local{};
  localtime_r(&seconds, &local);
  char stamp[32];
  std::strftime(stamp, sizeof(stamp), "%Y%m%d-%H%M%S", &local);
  return stamp;
}

std::string dump_path() {
  return (std::filesystem::temp_directory_path() / ("booth-dump-" + stamp_now() + ".json")).string();
}

std::optional<std::filesystem::path> changelog_file() {
  if (const char* configured = std::getenv("BOOTH_CHANGELOG");
      configured != nullptr && *configured != '\0') {
    const std::filesystem::path path(configured);
    if (std::filesystem::exists(path)) return path;
  }
  std::error_code ec;
  const auto cwd = std::filesystem::current_path(ec);
  if (!ec && std::filesystem::exists(cwd / "CHANGELOG.md")) return cwd / "CHANGELOG.md";
  const auto executable = std::filesystem::read_symlink("/proc/self/exe", ec);
  if (!ec) {
    const auto root = executable.parent_path().parent_path();
    for (const auto& candidate : {root / "CHANGELOG.md", root / "share" / "booth" / "CHANGELOG.md",
                                  std::filesystem::path("/usr/local/share/booth/CHANGELOG.md"),
                                  std::filesystem::path("/usr/share/booth/CHANGELOG.md")}) {
      if (std::filesystem::exists(candidate)) return candidate;
    }
  }
  return std::nullopt;
}

}  // namespace

bool handle_extra_command(AppState& state, const std::string& line, const std::string& cwd,
                          const std::filesystem::path& state_dir) {
  if (line == "/live" || line.rfind("/live ", 0) == 0) {
    state.notice = "live voice is unavailable: dsh exposes no audio transport";
    return true;
  }
  if (line == "/ratchet" || line.rfind("/ratchet ", 0) == 0) {
    state.notice = "ratchet is unavailable: dsh exposes no eval kernel";
    return true;
  }
  if (line == "/forget-approvals") {
    std::string error;
    state.notice = clear_approvals(state.approvals_path, error)
                       ? "Remembered approvals forgotten; the next read or write asks again."
                       : error;
    return true;
  }
  if (line == "/ssh" || line.rfind("/ssh ", 0) == 0) {
    const std::string_view args = line.size() > 4 ? std::string_view(line).substr(4) : std::string_view{};
    state.notice = handle_ssh_command(args, state_dir / "ssh.json",
                                      std::filesystem::path(cwd) / ".booth" / "ssh.json");
    return true;
  }
  if (line.rfind("/changelog", 0) == 0 &&
      (line.size() == 10 || (line.size() > 10 && line[10] == ' '))) {
    const auto path = changelog_file();
    if (!path) {
      state.notice = "changelog not found; set BOOTH_CHANGELOG";
      return true;
    }
    std::ifstream file(*path);
    if (!file.is_open()) {
      state.notice = "cannot read " + path->string();
      return true;
    }
    const std::string text((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
    const std::string_view args = line.size() > 10 ? std::string_view(line).substr(10) : std::string_view{};
    state.notice = format_changelog(args, parse_changelog(text));
    return true;
  }
  if (line == "/trace") {
    if (state.entries.empty()) {
      state.notice = "no transcript entries to trace";
      return true;
    }
    std::string name = state.session.empty() ? "session" : state.session;
    for (char& ch : name) {
      if (!std::isalnum(static_cast<unsigned char>(ch)) && ch != '-' && ch != '_' && ch != '.') ch = '_';
    }
    const auto path = state_dir / "traces" / (name + "-" + stamp_now() + ".html");
    std::string error;
    if (write_trace_report(state, path, error)) {
      open_url("file://" + path.string());
      state.notice = "trace report: " + path.string();
    } else {
      state.notice = error;
    }
    return true;
  }
  if (line == "/record") {
    if (recording()) {
      const std::string path = recording_path();
      const auto frames = recorded_frames();
      stop_recording();
      state.notice = "recording saved: " + path + " (" + std::to_string(frames) + " frames)";
    } else {
      const auto path = state_dir / "recordings" / ("booth-" + stamp_now() + ".jsonl");
      std::string error;
      state.notice = start_recording(path, error) ? "recording screen: " + path.string() : error;
    }
    return true;
  }
  if (line == "/debug") {
    state.notice = debug_text(state);
    return true;
  }
  if (line == "/extensions") {
    state.notice = extensions_text();
    return true;
  }
  if (line == "/plugins" || line.rfind("/plugins ", 0) == 0) {
    state.notice = plugins_text(line.size() > 8 ? std::string_view(line).substr(8) : std::string_view{});
    return true;
  }
  if (line == "/restart" || line == "/reload-plugins") {
    const bool reload = line == "/reload-plugins";
    if (state.busy) {
      state.notice = "wait for the current turn to finish";
      return true;
    }
    if (state.session.empty()) {
      if (!reload) {
        state.notice = "no active session to restart";
        return true;
      }
      state.restart_requested = true;
      state.notice = "reloading plugins (restarting booth)";
      return true;
    }
    std::string error;
    if (write_restart_marker(state_dir / "restart.json",
                             RestartMarker{state.session, cwd, state.title}, error)) {
      state.restart_requested = true;
      state.notice = reload ? "reloading plugins (restarting booth)" : "restarting booth";
    } else {
      state.notice = error;
    }
    return true;
  }
  if (line == "/open" || line.rfind("/open ", 0) == 0) {
    const std::string argument = trim(line.size() > 5 ? line.substr(5) : "");
    if (!argument.empty() && argument != "link" && argument != "url") {
      state.notice = "usage: /open [link]";
      return true;
    }
    auto links = collect_transcript_links(state.entries);
    if (links.empty()) {
      state.notice = "no links in the transcript";
      return true;
    }
    const auto& last = links.back();
    state.notice = (open_url(last.url) ? "opening " : "could not open ") + last.url;
    return true;
  }
  if (line == "/dump" || line.rfind("/dump ", 0) == 0) {
    const std::string mode = trim(line.size() > 5 ? line.substr(5) : "");
    if (!mode.empty() && mode != "all" && mode != "anon") {
      state.notice = "usage: /dump [all|anon]";
      return true;
    }
    const bool anonymized = mode == "anon";
    const std::string path = dump_path();
    std::ofstream output(path, std::ios::trunc);
    if (!output) {
      state.notice = "could not write " + path;
      return true;
    }
    output << dump_json(state, anonymized).dump(2) << '\n';
    output.close();
    if (!output) {
      state.notice = "could not write " + path;
      return true;
    }
    std::error_code error;
    std::filesystem::permissions(path,
                                 std::filesystem::perms::owner_read |
                                     std::filesystem::perms::owner_write,
                                 std::filesystem::perm_options::replace, error);
    std::string notice = "dumped transcript to " + path;
    if (!anonymized && copy_to_clipboard(transcript_markdown(state))) notice += " · copied";
    state.notice = std::move(notice);
    return true;
  }
  if (line == "/loop" || line.rfind("/loop ", 0) == 0) {
    loop_command(state, line, state.notice);
    return true;
  }
  if (line == "/mcp" || line.rfind("/mcp ", 0) == 0) {
    const auto words = split_words(line.size() > 4 ? std::string_view(line).substr(4) : std::string_view{});
    const std::filesystem::path file = state_dir / "mcp.json";
    if (words.empty() || words[0] == "list") {
      state.notice = mcp_list_text(file);
      return true;
    }
    if (words[0] == "add") {
      if (words.size() < 3) {
        state.notice =
            "usage: /mcp add <name> [--scope project|user] [--url <url>|-- <command...>]";
        return true;
      }
      const std::string name = words[1];
      std::string url;
      std::vector<std::string> command;
      bool after_dash = false;
      for (std::size_t index = 2; index < words.size(); ++index) {
        if (!after_dash && words[index] == "--") {
          after_dash = true;
          continue;
        }
        if (!after_dash && words[index] == "--url" && index + 1 < words.size()) {
          url = words[++index];
          continue;
        }
        if (!after_dash && words[index] == "--scope" && index + 1 < words.size()) {
          const std::string& scope = words[index + 1];
          if (scope != "project" && scope != "user") {
            state.notice = "usage: /mcp add <name> [--scope project|user] [--url <url>|-- <command...>]";
            return true;
          }
          ++index;  // Scope is accepted for OMP parity; this store is profile-local.
          continue;
        }
        if (!after_dash && words[index].rfind("--", 0) == 0) {
          state.notice = "unknown /mcp add option " + words[index] +
                         " · use -- before a command that starts with '-'";
          return true;
        }
        command.push_back(words[index]);
      }
      if (!url.empty() && !command.empty()) {
        state.notice = "usage: /mcp add <name> [--url <url>|-- <command...>]";
        return true;
      }
      if (url.empty() && command.empty()) {
        state.notice = "usage: /mcp add <name> <command...|url>";
        return true;
      }
      nlohmann::json server;
      if (!url.empty() || (command.size() == 1 &&
                           (command[0].rfind("http://", 0) == 0 ||
                            command[0].rfind("https://", 0) == 0))) {
        server["url"] = url.empty() ? command[0] : url;
      } else {
        server["command"] = command[0];
        nlohmann::json args = nlohmann::json::array();
        for (std::size_t index = 1; index < command.size(); ++index) args.push_back(command[index]);
        server["args"] = std::move(args);
      }
      const auto saved = upsert_mcp_server(file, name, std::move(server));
      state.notice = saved.is_null() ? "could not write " + file.string()
                                     : "mcp " + name + " saved · /new starts a session with it";
      return true;
    }
    if (words[0] == "remove" || words[0] == "rm") {
      if (words.size() < 2) {
        state.notice = "usage: /mcp remove <name>";
        return true;
      }
      state.notice = remove_mcp_server(file, words[1]) ? "mcp " + words[1] + " removed"
                                                       : "no mcp server " + words[1];
      return true;
    }
    if (words[0] == "test") {
      if (words.size() < 2) {
        state.notice = "usage: /mcp test <name>";
        return true;
      }
      state.notice = mcp_test_text(file, words[1]);
      return true;
    }
    if (words[0] == "enable" || words[0] == "disable") {
      if (words.size() < 2) {
        state.notice = "usage: /mcp " + words[0] + " <name>";
        return true;
      }
      const bool enabled = words[0] == "enable";
      state.notice = set_mcp_enabled(file, words[1], enabled)
                         ? "mcp " + words[1] + (enabled ? " enabled" : " disabled") +
                               " · /new starts a session with the change"
                         : "no mcp server " + words[1];
      return true;
    }
    if (words[0] == "reload" || words[0] == "reconnect") {
      state.notice = words[0] == "reload"
                         ? "MCP servers reload when a session starts; run /new or /fresh"
                         : "MCP reconnects when a session starts; run /new or /fresh";
      return true;
    }
    if (words[0] == "help") {
      state.notice =
          "Usage: /mcp [list|add <name> <command...|url>|remove <name>|test <name>|enable <name>|disable <name>|reload]";
      return true;
    }
    if (words[0] == "reauth" || words[0] == "unauth" || words[0] == "resources" ||
        words[0] == "prompts" || words[0] == "notifications") {
      state.notice = "mcp " + words[0] + " is unavailable: dsh owns MCP auth and live server state";
      return true;
    }
    state.notice = "usage: /mcp [list|add <name> [--url <url>|-- <cmd...>]|remove|test|enable|disable|reload]";
    return true;
  }
  if (line == "/local" || line.rfind("/local ", 0) == 0) {
    const std::string target_name = trim(line.size() > 6 ? line.substr(6) : "");
    if (!target_name.empty()) {
      if (target_name.find('/') != std::string::npos || target_name == "." || target_name == "..") {
        state.notice = "usage: /local [file]";
        return true;
      }
      std::error_code error;
      const auto base = std::filesystem::canonical(state_dir, error);
      const auto target = std::filesystem::canonical(state_dir / target_name, error);
      if (error || target.parent_path() != base) {
        state.notice = "no local file " + target_name;
        return true;
      }
      state.notice = (open_path(target) ? "opened " : "could not open ") + target.string();
      return true;
    }
    std::vector<std::string> names;
    std::error_code error;
    for (const auto& entry : std::filesystem::directory_iterator(state_dir, error)) {
      if (!entry.is_regular_file()) continue;
      names.push_back(entry.path().filename().string());
    }
    if (names.empty()) {
      state.notice = "no local files in " + state_dir.string();
      return true;
    }
    std::sort(names.begin(), names.end());
    std::string out = "local files · " + state_dir.string();
    for (const auto& name : names) out += " · " + name;
    state.notice = std::move(out);
    return true;
  }
  return false;
}

}  // namespace booth
