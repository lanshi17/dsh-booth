#include "interaction/control_commands.hpp"
#include "interaction/btw_history.hpp"
#include "interaction/command_parsing.hpp"
#include "interaction/control_views.hpp"
#include "interaction/todo_commands.hpp"
#include "interaction/git_commands.hpp"
#include "interaction/open_link.hpp"
#include "interaction/preset_store.hpp"
#include "interaction/state.hpp"

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <fstream>
#include <optional>
#include <string>
#include <utility>
#include <nlohmann/json.hpp>
namespace booth {
namespace {

using json = nlohmann::json;
using namespace std::chrono_literals;
std::string_view first_word(std::string_view line) {
  const auto end = line.find_first_of(" \t\r\n");
  return line.substr(0, end);
}

std::string argument_after(std::string_view line) {
  const auto end = line.find_first_of(" \t");
  if (end == std::string_view::npos) return {};
  const auto first = line.find_first_not_of(" \t", end);
  if (first == std::string_view::npos) return {};
  return std::string(line.substr(first));
}

std::string trim(std::string text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return text.substr(first, last - first + 1);
}

TranscriptEntry command_entry(std::string label, std::string text, bool failed = false) {
  TranscriptEntry entry;
  entry.kind = EntryKind::Command;
  entry.tool = std::move(label);
  entry.text = std::move(text);
  if (failed) entry.status = "failed";
  return entry;
}

std::string command_result_text(const json& result) {
  if (result.is_object() && result.contains("result") && result["result"].is_object()) {
    const auto& inner = result["result"];
    return inner.value("text", std::string{"(no output)"});
  }
  return result.dump(2);
}
std::string error_text(const Error& error) {
  if (error.layer == Layer::Protocol) return error.detail;
  return to_string(error);
}

/// Post one request; format its result or its failure on the transcript.
void submit(const ControlHooks& hooks, std::string label, std::string method, json params,
            std::chrono::milliseconds timeout,
            std::function<std::string(const json&)> format) {
  if (hooks.control == nullptr) {
    hooks.publish(command_entry(std::move(label), "control channel unavailable", true));
    return;
  }
  hooks.post([hooks, label = std::move(label), method = std::move(method), params = std::move(params),
              timeout, format = std::move(format)]() mutable {
    auto result = hooks.control->request(method, params, timeout);
    if (!result) {
      hooks.publish(command_entry(std::move(label), error_text(result.error()), true));
      return;
    }
    std::string text;
    try {
      text = format(*result);
    } catch (const std::exception& error) {
      text = std::string("cannot read the control reply: ") + error.what();
    }
    hooks.publish(command_entry(std::move(label), std::move(text)));
  });
}

void fork_at(const ControlHooks& hooks, const std::string& session,
             std::optional<std::int64_t> at_seq, std::string label) {
  if (hooks.control == nullptr) {
    hooks.publish(command_entry(std::move(label), "control channel unavailable", true));
    return;
  }
  hooks.post([hooks, session, at_seq, label = std::move(label)]() mutable {
    json params = {{"sessionId", session}};
    if (at_seq) params["atSeq"] = *at_seq;
    auto result = hooks.control->request("session.fork", params, 120s);
    if (!result) {
      hooks.publish(command_entry(std::move(label), error_text(result.error()), true));
      return;
    }
    const std::string child = result->value("sessionId", std::string{});
    if (child.empty()) {
      hooks.publish(command_entry(std::move(label), "the control reply named no child session", true));
      return;
    }
    hooks.publish(command_entry(label, "forked session " + child));
    if (hooks.switch_session) {
      SessionRow row;
      row.id = SessionId{child};
      row.title = result->value("title", std::string{});
      row.cwd = result->value("cwd", std::string{});
      hooks.switch_session(row);
    }
  });
}

}  // namespace

bool is_control_command(std::string_view line) {
  const auto word = first_word(line);
  return word == "/compact" || word == "/goal" || word == "/plan" || word == "/rename" ||
         word == "/fork" || word == "/branch" || word == "/tree" || word == "/delete" || word == "/jobs" ||
         word == "/git" || word == "/commit" || word == "/shake" || word == "/modelpreset" ||
         word == "/plugins" || word == "/session" || word == "/tools" || word == "/agents" || word == "/add-dir" ||
         word == "/remove-dir" || word == "/dirs" || word == "/fresh" || word == "/move" ||
         word == "/wt" || word == "/btw" || word == "/tan" || word == "/guided-goal" ||
         word == "/handoff" || word == "/memory" || word == "/marketplace" || word == "/cleanse" ||
         word == "/security" || word == "/prewalk" || word == "/omfg" || word == "/advisor" ||
         word == "/login" || word == "/logout" || word == "/browser" || word == "/computer" ||
         word == "/plan-review" || word == "/force" || word.rfind("/force:", 0) == 0 ||
         word == "/skillful" || word == "/extended-context" || word == "/share" || word == "/collab" ||
         word == "/join" || word == "/leave" || word == "/fast" || word == "/slow" || word == "/vibe" ||
         word == "/todo";
}

bool handle_control_command(const ControlHooks& hooks, std::string_view line,
                            const std::string& session) {
  const auto word = first_word(line);
  if (word == "/todo" && handle_todo_control(hooks, line, session)) return true;
  if (word == "/compact") {
    const auto request = parse_compact_request(argument_after(line));
    if (!request.error.empty()) {
      hooks.publish(command_entry("compact", request.error, true));
      return true;
    }
    const std::string prefix = request.prefix;
    submit(hooks, "compact", "command.execute",
           {{"sessionId", session}, {"line", request.line}}, 600s,
           [prefix](const json& result) { return prefix + command_result_text(result); });
    return true;
  }
  if (word == "/vibe") { submit(hooks, "vibe", "vibe", {{"sessionId", session}, {"args", trim(argument_after(line))}}, 300s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/fast" || word == "/slow") { submit(hooks, std::string(word.substr(1)), std::string(word.substr(1)), {{"sessionId", session}, {"args", trim(argument_after(line))}}, 120s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/share" || word == "/collab" || word == "/join" || word == "/leave") { submit(hooks, std::string(word.substr(1)), std::string(word.substr(1)), {{"sessionId", session}, {"args", trim(argument_after(line))}}, 120s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/extended-context") { submit(hooks, "extended-context", "extended-context", {{"sessionId", session}, {"args", trim(argument_after(line))}}, 120s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/skillful") { submit(hooks, "skillful", "skillful", {{"sessionId", session}, {"args", trim(argument_after(line))}}, 120s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/force" || word.rfind("/force:", 0) == 0) {
    std::string args = trim(argument_after(line));
    if (word.rfind("/force:", 0) == 0) args = std::string(word.substr(7)) + (args.empty() ? "" : " " + args);
    submit(hooks, "force", "force", {{"sessionId", session}, {"args", args}}, 120s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
  if (word == "/plan-review") {
    submit(hooks, "plan-review", "plan-review", {{"sessionId", session}}, 300s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/browser" || word == "/computer") {
    const std::string feature = word == "/browser" ? "browser" : "computer";
    submit(hooks, feature, feature, {{"args", trim(argument_after(line))}}, 300s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/login") {
    submit(hooks, "login", "login",
           {{"sessionId", session}, {"args", trim(argument_after(line))}}, 30s, [](const json& result) {
      const std::string url = result.value("url", std::string{});
      if (!url.empty()) open_url(url);
      return result.value("text", result.dump(2));
    });
    return true;
  }
  if (word == "/logout") {
    submit(hooks, "logout", "logout",
           {{"sessionId", session}, {"args", trim(argument_after(line))}}, 60s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/advisor") {
    submit(hooks, "advisor", "advisor", {{"sessionId", session}, {"args", trim(argument_after(line))}},
           120s, [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/omfg") {
    submit(hooks, "omfg", "omfg", {{"sessionId", session}, {"complaint", trim(argument_after(line))}},
           300s, [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/prewalk") {
    submit(hooks, "prewalk", "prewalk", {{"sessionId", session}, {"args", trim(argument_after(line))}},
           120s, [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/security") {
    submit(hooks, "security", "security", {{"sessionId", session}, {"args", trim(argument_after(line))}},
           120s, [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/cleanse") {
    submit(hooks, "cleanse", "cleanse", {{"sessionId", session}, {"request", trim(argument_after(line))}},
           900s, [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/marketplace") {
    submit(hooks, "marketplace", "marketplace", {{"args", trim(argument_after(line))}}, 120s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/memory") {
    submit(hooks, "memory", "memory", {{"args", trim(argument_after(line))}}, 120s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/handoff") {
    const bool has_focus = !trim(argument_after(line)).empty();
    submit(hooks, "handoff", "command.execute",
           {{"sessionId", session}, {"line", "/compact"}}, 600s, [has_focus](const json& result) {
             const std::string prefix = has_focus
                 ? "handoff checkpoint (focus instructions ignored): "
                 : "handoff checkpoint: ";
             return prefix + command_result_text(result);
           });
    return true;
  }
  if (word == "/goal") {
    const auto request = parse_goal_request(argument_after(line));
    if (!request.error.empty()) {
      hooks.publish(command_entry("goal", request.error, true));
      return true;
    }
    submit(hooks, "goal", "command.execute",
           {{"sessionId", session}, {"line", request.line}}, 120s, command_result_text);
    return true;
  }
  if (word == "/plan") {
    const std::string argument = trim(argument_after(line));
    if (!argument.empty()) {
      submit(hooks, "plan", "command.execute",
             {{"sessionId", session}, {"line", "/plan " + argument}}, 120s, command_result_text);
      return true;
    }
    if (hooks.control == nullptr) {
      hooks.publish(command_entry("plan", "control channel unavailable", true));
      return true;
    }
    hooks.post([hooks, session] {
      auto status = hooks.control->request("plan.status", {{"sessionId", session}}, 30s);
      if (!status) {
        hooks.publish(command_entry("plan", error_text(status.error()), true));
        return;
      }
      const bool active = status->value("active", false);
      auto changed = hooks.control->request("plan.set", {{"sessionId", session}, {"active", !active}}, 30s);
      if (!changed) {
        hooks.publish(command_entry("plan", error_text(changed.error()), true));
        return;
      }
      hooks.publish(command_entry("plan", active ? "plan mode off" : "plan mode on"));
    });
    return true;
  }
  if (word == "/rename") {
    submit(hooks, "rename", "session.rename",
           {{"sessionId", session}, {"title", trim(argument_after(line))}}, 120s, [hooks](const json& result) {
             const std::string title = result.value("title", std::string{});
             if (!title.empty() && hooks.title_changed) hooks.title_changed(title);
             return title.empty() ? "renamed" : "renamed to " + title;
           });
    return true;
  }
  if (word == "/fork" || word == "/branch") {
    const std::string argument = trim(argument_after(line));
    if (word == "/branch" && (argument.empty() || argument == "list" || argument == "show")) {
      submit(hooks, "tree", "session.events", {{"sessionId", session}}, 30s, tree_text);
      return true;
    }
    std::optional<std::int64_t> at_seq;
    const bool placement = argument == "pane" || argument == "window" || argument == "tab";
    if (!argument.empty() && !(word == "/fork" && placement)) {
      try {
        at_seq = std::stoll(argument);
      } catch (const std::exception&) {
        hooks.publish(command_entry(std::string(word),
                                    word == "/fork" ? "usage: /fork [pane|window|tab]"
                                                    : "usage: /branch [seq|list]",
                                    true));
        return true;
      }
    }
    if (word == "/fork" && placement) {
      hooks.publish(command_entry("fork", "multiplexer placement is unavailable; forking the session"));
    }
    fork_at(hooks, session, at_seq, word == "/branch" ? "branch" : "fork");
    return true;
  }
  if (word == "/tree") {
    submit(hooks, "tree", "session.events", {{"sessionId", session}}, 30s, tree_text);
    return true;
  }
  if (word == "/delete") {
    if (hooks.control == nullptr || !hooks.delete_session) {
      hooks.publish(command_entry("delete", "control channel unavailable", true));
      return true;
    }
    hooks.post([hooks, session] { hooks.delete_session(session); });
    return true;
  }
  if (word == "/shake") {
    const std::string mode = trim(argument_after(line));
    submit(hooks, "shake", "session.shake",
           {{"sessionId", session}, {"mode", mode.empty() ? "elide" : mode}}, 60s, [](const json& result) {
             const std::int64_t pruned = result.value("pruned", std::int64_t{0});
             const std::int64_t removed = result.value("charsRemoved", std::int64_t{0});
             if (pruned == 0) return std::string("nothing to elide");
             return "elided " + std::to_string(pruned) + " tool result" +
                    (pruned == 1 ? "" : "s") + " · " + std::to_string(removed) + " chars";
           });
    return true;
  }
  if (word == "/session") {
    const std::string argument = trim(argument_after(line));
    const auto split = argument.find_first_of(" \t"); const std::string action = argument.substr(0, split); const std::string rest = split == std::string::npos ? std::string{} : trim(argument.substr(split + 1));
    if (action.empty()) return false;  // local view owns the one-line summary
    if (action == "info") {
      if (rest.empty()) return false;
      hooks.publish(command_entry("session", "usage: /session [info|delete|pin [account]]", true));
      return true;
    }
    if (action == "delete") {
      if (!rest.empty()) {
        hooks.publish(command_entry("session", "usage: /session delete", true));
        return true;
      }
      if (hooks.state != nullptr && hooks.state->busy) {
        hooks.publish(command_entry("session", "wait for the current turn to finish", true));
        return true;
      }
      if (!hooks.delete_session) {
        hooks.publish(command_entry("session", "session deletion is unavailable", true));
        return true;
      }
      hooks.post([hooks, session] { hooks.delete_session(session); });
      return true;
    }
    if (action == "pin") {
      hooks.publish(command_entry(
          "session",
          "account pin is unavailable: dsh manages OAuth credentials outside the session"));
      return true;
    }
    hooks.publish(command_entry("session", "usage: /session [info|delete|pin [account]]", true));
    return true;
  }
  if (word == "/jobs") {
    const std::string argument = trim(argument_after(line));
    if (argument.rfind("kill ", 0) == 0) {
      const std::string id = trim(argument.substr(5));
      if (id.empty()) {
        hooks.publish(command_entry("jobs", "usage: /jobs [full|kill <id>|kill all]", true));
        return true;
      }
      submit(hooks, "jobs", "jobs.kill", {{"sessionId", session}, {"id", id}}, 30s,
             [id](const json& result) {
               const std::string text = result.value("text", std::string{}); if (!text.empty()) return text;
               const auto killed = result.value("killed", json::array());
               if (id == "all") {
                 return killed.empty() ? std::string("no running jobs to kill")
                                       : "killed " + std::to_string(killed.size()) + " job(s)";
               }
               return "kill requested for " + id;
             });
      return true;
    }
    if (!argument.empty() && argument != "full") {
      hooks.publish(command_entry("jobs", "usage: /jobs [full|kill <id>|kill all]", true));
      return true;
    }
    const bool full = argument == "full";
    submit(hooks, "jobs", "jobs.list", {{"sessionId", session}}, 30s,
           [full](const json& result) { return jobs_text(result, full); });
    return true;
  }
  if (word == "/tools") {
    submit(hooks, "tools", "tools.list", {{"sessionId", session}}, 30s, tools_text);
    return true;
  }
  if (word == "/agents") {
    submit(hooks, "agents", "agents.list", {{"sessionId", session}}, 30s, agents_text);
    return true;
  }
  if (word == "/modelpreset") {
    if (hooks.state == nullptr) {
      hooks.publish(command_entry(std::string(word), "no application state", true));
      return true;
    }
    const auto file = hooks.state_dir / "model-presets.json";
    const std::string argument = trim(argument_after(line));
    const auto space = argument.find_first_of(" \t");
    std::string action = space == std::string::npos ? argument : argument.substr(0, space);
    std::string name = space == std::string::npos ? std::string{} : trim(argument.substr(space + 1));
    if (action.empty() || action == "list") {
      hooks.publish(command_entry(std::string(word), model_preset_list(file)));
      return true;
    }
    if (action == "save") {
      if (name.empty()) {
        hooks.publish(command_entry("modelpreset", "usage: /modelpreset save <name>", true));
        return true;
      }
      ModelPreset preset;
      preset.name = name;
      preset.model_value = hooks.state->model;
      preset.model_name = hooks.state->model;
      for (const auto& choice : hooks.state->models) {
        if (choice.name == hooks.state->model || choice.value == hooks.state->model) {
          preset.model_value = choice.value;
          preset.model_name = choice.name;
          break;
        }
      }
      preset.effort_id = hooks.state->reasoning.id;
      preset.effort_value = hooks.state->reasoning.current;
      preset.effort_name = hooks.state->reasoning.current_name;
      hooks.publish(command_entry("modelpreset",
                                  save_model_preset(file, preset)
                                      ? "saved preset " + name + " · " + preset.model_name
                                      : "could not write " + file.string(),
                                  false));
      return true;
    }
    if (action == "delete" || action == "rm") {
      if (name.empty()) {
        hooks.publish(command_entry("modelpreset", "usage: /modelpreset delete <name>", true));
        return true;
      }
      hooks.publish(command_entry("modelpreset",
                                  remove_model_preset(file, name) ? "deleted preset " + name
                                                                  : "no preset " + name,
                                  false));
      return true;
    }
    if (action == "switch") {
      if (name.empty()) {
        hooks.publish(command_entry("modelpreset", "usage: /modelpreset switch <name>", true));
        return true;
      }
      const auto preset = find_model_preset(file, name);
      if (!preset) {
        hooks.publish(command_entry("modelpreset", "no preset " + name, true));
        return true;
      }
      if (hooks.apply_model) hooks.apply_model(preset->model_value, preset->model_name);
      if (!preset->effort_id.empty() && hooks.apply_reasoning) {
        hooks.apply_reasoning(preset->effort_id,
                              ModelChoice{"", preset->effort_name, preset->effort_value});
      }
      hooks.publish(command_entry("modelpreset", "applied preset " + name + " · " + preset->model_name +
                                                  (preset->effort_name.empty()
                                                       ? ""
                                                       : " · " + preset->effort_name)));
      return true;
    }
    hooks.publish(command_entry("modelpreset",
                                "usage: /modelpreset [list|save <name>|delete <name>|switch <name>]",
                                true));
    return true;
  }
  if (word == "/plugins") {
    const std::string argument = trim(argument_after(line));
    const auto split = argument.find_first_of(" \t");
    const std::string action = argument.substr(0, split);
    if (action.empty() || action == "list") { if (hooks.control == nullptr) return false; submit(hooks, "plugins", "plugins.list", json::object(), 30s, [](const json& result) { return result.value("text", result.dump(2)); }); return true; }
    if (action != "enable" && action != "disable") return false;
    const std::string name = split == std::string::npos ? std::string{} : trim(argument.substr(split + 1));
    if (name.empty()) {
      hooks.publish(command_entry("plugins", "usage: /plugins " + action + " <plugin>", true));
      return true;
    }
    const bool enabled = action == "enable";
    submit(hooks, "plugins", "plugins.set", {{"name", name}, {"enabled", enabled}}, 120s,
           [](const json& result) { return result.value("text", result.dump(2)); });
    return true;
  }
  if (word == "/wt") {
    if (hooks.state != nullptr && hooks.state->busy) {
      hooks.publish(command_entry("wt", "wait for the current turn to finish", true));
      return true;
    }
    if (!hooks.worktree_session) {
      hooks.publish(command_entry("wt", "worktree move is unavailable", true));
      return true;
    }
    hooks.publish(command_entry("wt", "creating a worktree"));
    hooks.worktree_session(session, trim(argument_after(line)));
    return true;
  }
  if (word == "/move") {
    if (hooks.state != nullptr && hooks.state->busy) {
      hooks.publish(command_entry("move", "wait for the current turn to finish", true));
      return true;
    }
    const std::string path = trim(argument_after(line));
    if (path.empty()) {
      hooks.publish(command_entry("move", "usage: /move <path>", true));
      return true;
    }
    if (!hooks.move_session) {
      hooks.publish(command_entry("move", "move is unavailable", true));
      return true;
    }
    hooks.publish(command_entry("move", "moving session to " + path));
    hooks.move_session(session, path);
    return true;
  }
  if (word == "/guided-goal") {
    submit(hooks, "guided-goal", "goal.guided",
           {{"sessionId", session}, {"rough", trim(argument_after(line))}}, 30s, [](const json&) {
             return "guided goal interview started";
           });
    return true;
  }
  if (word == "/btw") {
    const std::string question = trim(argument_after(line));
    if (question.empty()) {
      hooks.publish(command_entry("btw", btw_history_text(hooks.state_dir / "btw-history.jsonl", session)));
      return true;
    }
    const auto history = hooks.state_dir / "btw-history.jsonl";
    submit(hooks, "btw", "side.ask", {{"sessionId", session}, {"text", question}}, 600s,
           [history, session, question](const json& result) {
             std::string text = result.value("output", std::string{});
             const std::string stop = result.value("stopReason", std::string{});
             const std::string diagnostic = result.value("diagnostic", std::string{});
             if (text.empty()) text = stop == "completed" ? "(no answer)" : "side agent ended: " + stop;
             if (!diagnostic.empty()) text += "\n" + diagnostic;
             append_btw_history(history, session, question, text);
             return text;
           });
    return true;
  }
  if (word == "/tan") {
    const std::string work = trim(argument_after(line));
    if (work.empty()) {
      hooks.publish(command_entry("tan", "usage: /tan <work>", true));
      return true;
    }
    submit(hooks, "tan", "side.background", {{"sessionId", session}, {"text", work}}, 60s,
           [](const json& result) {
             const std::string child = result.value("childId", std::string{});
             return child.empty() ? "background agent started" : "background agent " + child;
           });
    return true;
  }
  if (word == "/fresh") {
    if (hooks.state != nullptr && hooks.state->busy) {
      hooks.publish(command_entry("fresh", "wait for the current turn to finish", true));
    } else if (hooks.fresh_session) {
      hooks.publish(command_entry("fresh", "refreshing session state"));
      hooks.fresh_session();
    } else {
      hooks.publish(command_entry("fresh", "fresh is unavailable", true));
    }
    return true;
  }
  if (word == "/dirs") {
    submit(hooks, "dirs", "dirs.list", {{"sessionId", session}}, 30s, dirs_text);
    return true;
  }
  if (word == "/add-dir" || word == "/remove-dir") {
    const std::string path = trim(argument_after(line));
    if (path.empty()) {
      hooks.publish(command_entry(std::string(word), "usage: " + std::string(word) + " <absolute path>", true));
      return true;
    }
    const std::string method = word == "/add-dir" ? "dirs.add" : "dirs.remove";
    submit(hooks, std::string(word).substr(1), method, {{"sessionId", session}, {"path", path}}, 30s,
           [](const json& result) {
             const auto roots = result.value("roots", json::array());
             return result.value("path", std::string{}) + " · " +
                    std::to_string(roots.is_array() ? roots.size() : 0) + " added director" +
                    (roots.is_array() && roots.size() == 1 ? "y" : "ies");
           });
    return true;
  }
  if (is_git_command(line)) return handle_git_command(hooks, line);
  return false;
}

}  // namespace booth
