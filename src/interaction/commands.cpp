#include "interaction/commands.hpp"

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <iterator>
#include <map>
#include <utility>
#include <vector>

#include "interaction/file_command.hpp"

namespace booth {

const CommandHint kCommandHints[] = {
    {"/help", "keys and commands", "shortcuts keys help", "hotkeys keys"},
    {"/model", "switch the session model", "provider choose config", "models"},
    {"/status", "phase, session and model", "info state session", ""},
    {"/notify", "terminal notifications on or off", "alerts attention bell", ""},
    {"/theme", "reload or set live colors", "palette color accent", ""},
    {"/context", "context window usage", "tokens window percent usage", ""},
    {"/stats", "transcript and tool counters", "counts entries tools", ""},
    {"/copy", "copy an answer, code, command, link or the last prompt", "clipboard yank", ""},
    {"/links", "open a transcript URL", "link url browser hint", ""},
    {"/mouse", "terminal/tmux mouse capture", "scroll select copy tmux", ""},
    {"/clear", "clear the local transcript", "reset erase screen", ""},
    {"/retry", "send the last prompt again", "again rerun repeat", ""},
    {"/export", "export the transcript as HTML or Markdown", "save file html markdown", ""},
    {"/sessions", "resume a saved session", "list history resume", "resume"},
    {"/skills", "search, list or invoke dsh skills", "skill invoke catalog", "skill"},
    {"/providers", "configure providers", "provider setup credentials auth", "provider setup"},
    {"/settings", "toggle booth interaction settings", "preferences options interface", ""},
    {"/new", "start a fresh session", "fresh reset close", ""},
    {"/effort", "set the reasoning effort", "reasoning thinking level", "reasoning"},
    {"/queue", "queue a prompt while a run is active", "later after busy", ""},
    {"/compact", "compact the conversation context", "condense summarize history", ""},
    {"/handoff", "compact with a handoff checkpoint", "handoff checkpoint compact", ""},
    {"/goal", "set, show or manage the session goal", "objective pause resume drop", ""},
    {"/plan", "toggle plan mode", "planning review approve", ""},
    {"/rename", "rename the session, or generate a title", "title name", ""},
    {"/fork", "fork this session and switch to the child", "branch copy duplicate", ""},
    {"/delete", "delete this session and start a new one", "remove erase close", ""},
    {"/commit", "stage and commit changes", "git save changes", ""},
    {"/modelpreset", "save, list or delete model presets", "preset model effort", ""},
    {"/loop", "repeat a prompt until a count, duration or condition ends it", "repeat while until", ""},
    {"/add-dir", "grant writes under an extra directory", "workspace root folder", ""},
    {"/remove-dir", "remove an extra writable directory", "workspace root folder", ""},
    {"/dirs", "list the workspace and extra directories", "workspace roots folders", ""},
    {"/fresh", "reset session stream state without clearing history", "reload reconnect", ""},
    {"/move", "move this session to another directory", "relocate folder", ""},
    {"/wt", "move this session to a git worktree", "worktree branch", "worktree"},
    {"/restart", "restart booth and resume this session", "relaunch reload", ""},
    {"/reload-plugins", "reload profile plugins by restarting", "plugins restart reload", ""},
    {"/btw", "ask a side question in a separate agent", "side question", ""},
    {"/tan", "run a background agent on tangential work", "background agent tangent", ""},
    {"/guided-goal", "interview for and create a verifiable goal", "goal interview objective", ""},
    {"/debug", "show a diagnostic state snapshot", "diagnostics state", ""},
    {"/extensions", "show the dsh profile bundles and patch rows", "plugins bundles profile", ""},
    {"/plugins", "list profile plugins and bundles", "plugins bundles dependencies", "plugin"},
    {"/record", "record the rendered screen to JSONL", "screen recording capture", ""},
    {"/changelog", "show release notes", "releases history news", ""},
    {"/trace", "write a session trace report", "trace stats dashboard report", ""},
    {"/ssh", "manage configured SSH hosts", "ssh hosts remote", ""},
    {"/pause", "freeze or resume the local kernel", "pause resume freeze agents", ""},
    {"/forget-approvals", "forget remembered permission grants", "approvals grants remember", ""},
    {"/memory", "inspect and maintain local memory", "memory stats sync clear", ""},
    {"/marketplace", "manage profile plugin sources", "marketplace plugins install", ""},
    {"/cleanse", "detect and fix project diagnostics", "cleanse diagnostics repair", ""},
    {"/security", "run and inspect security audits", "security scan findings", ""},
    {"/prewalk", "arm a one-shot model handoff", "prewalk model handoff", ""},
    {"/omfg", "forge a durable behavioral rule", "omfg rule complaint", ""},
    {"/advisor", "toggle or inspect the review advisor", "advisor review notes", ""},
    {"/login", "sign in to the DeepSeek account", "login account auth", ""},
    {"/logout", "sign out of the DeepSeek account", "logout account auth", ""},
    {"/browser", "manage the browser-use provider", "browser playwright headless", ""},
    {"/computer", "manage the computer-use provider", "computer cua desktop", ""},
    {"/plan-review", "review the active plan with a subagent", "plan review critique", ""},
    {"/force", "restrict the next turn to one tool", "force tool choice", ""},
    {"/skillful", "toggle session skill listing", "skillful skills listing", ""},
    {"/extended-context", "switch to the largest context model", "extended context long window", ""},
    {"/share", "start a local shared session server", "share liveshare viewer", ""},
    {"/collab", "toggle shared-session write access", "collab share write", ""},
    {"/join", "open an active or local share URL", "join share viewer", ""},
    {"/leave", "stop sharing this session", "leave share stop", ""},
    {"/fast", "toggle provider priority service tier", "fast priority service", ""},
    {"/slow", "toggle provider flex service tier", "slow flex service", ""},
    {"/vibe", "ask a read-only vibe worker", "vibe worker read-only", ""},
    {"/switch", "switch the session model without changing the saved selection", "model temporary session", ""},
    {"/shake", "elide heavy tool results from context", "prune shrink images thinking", ""},
    {"/branch", "list turns or fork at a sequence", "rewind tree history", "rewind"},
    {"/tree", "list the session event tree", "branch history turns", ""},
    {"/jobs", "background jobs, full output or kill", "job kill output", ""},
    {"/agents", "live agents in this process", "subagents roster hub", "subagents hub"},
    {"/session", "info or delete the active ACP session", "info id title workspace", ""},
    {"/acp", "show the ACP connection", "protocol socket phase transport", ""},
    {"/cancel", "cancel the active run", "stop abort interrupt", ""},
    {"/find", "search the transcript", "grep locate text", ""},
    {"/attach", "attach a file as an ACP resource link", "file resource prompt", ""},
    {"/detach", "clear attached resource links", "remove file resource", ""},
    {"/close", "close the active session", "end session", ""},
    {"/todo", "show the agent todo list", "tasks checklist progress", ""},
    {"/tools", "show tool activity counts", "tool calls counters", ""},
    {"/usage", "context and transcript usage", "tokens context stats", ""},
    {"/git", "working tree branch and changes", "status branch dirty", ""},
    {"/pin", "pin the active session", "sticky top resume", ""},
    {"/unpin", "unpin the active session", "remove pin resume", ""},
    {"/paste", "paste clipboard text raw", "paste clipboard raw", ""},
    {"/quit", "leave booth", "exit quit", "exit q"},
    {"/open", "open the last transcript link", "url browser", ""},
    {"/dump", "write the transcript JSON and copy markdown", "save json clipboard", ""},
    {"/local", "list booth local files", "state files", ""},
    {"/mcp", "manage MCP servers for new sessions", "mcp add remove list test enable server", ""},
    {"/live", "realtime voice is unavailable on dsh", "voice audio speech", ""},
    {"/ratchet", "eval hillclimbing is unavailable on dsh", "evals improve baseline", ""},
};

namespace {

std::string lower_ascii(std::string_view text);

std::map<std::string, FileCommand> gFileCommands;
std::map<std::string, SkillInfo> gSkills;

std::vector<CommandHint> build_hints() {
  std::vector<CommandHint> hints(std::begin(kCommandHints), std::end(kCommandHints));
  for (const auto& [name, command] : gFileCommands) {
    hints.push_back(CommandHint{"/" + name, command.description, "project command template", ""});
  }
  for (const auto& [name, skill] : gSkills) {
    hints.push_back(CommandHint{"/" + name, skill.description, "skill", "", true});
  }
  return hints;
}

std::string lower_ascii(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

int subsequence_score(std::string_view query, std::string_view target) {
  if (query.size() > target.size()) return 0;
  std::size_t matched = 0;
  int gaps = 0;
  int last = -1;
  for (int index = 0; index < static_cast<int>(target.size()) && matched < query.size(); ++index) {
    if (target[static_cast<std::size_t>(index)] != query[matched]) continue;
    if (last >= 0 && index - last > 1) ++gaps;
    last = index;
    ++matched;
  }
  if (matched != query.size()) return 0;
  return std::max(1, 40 - gaps * 5);
}

int text_score(std::string_view query, std::string_view target) {
  if (query.empty()) return 1;
  if (target == query) return 100;
  if (target.rfind(query, 0) == 0) return 80;
  if (target.find(query) != std::string_view::npos) return 60;
  return subsequence_score(query, target);
}

template <class Fn>
void for_each_alias(std::string_view aliases, Fn&& fn) {
  std::string_view rest(aliases);
  while (!rest.empty()) {
    const auto space = rest.find(' ');
    const std::string_view alias = rest.substr(0, space);
    if (!alias.empty()) fn(alias);
    if (space == std::string_view::npos) break;
    rest.remove_prefix(space + 1);
  }
}

/// Lowercased search text for one hint, rebuilt with the hint list instead of per keystroke.
struct HintKey {
  std::string name;
  std::vector<std::string> aliases;
  std::string text;  // description and keywords; empty for skills
  bool skill{false};
};

HintKey make_key(const CommandHint& hint) {
  HintKey key;
  key.name = lower_ascii(std::string_view(hint.name).substr(1));
  key.skill = hint.skill;
  for_each_alias(hint.aliases, [&](std::string_view alias) { key.aliases.push_back(lower_ascii(alias)); });
  if (!key.skill) key.text = lower_ascii(std::string(hint.description) + " " + hint.keywords);
  return key;
}

std::vector<CommandHint> gHints = build_hints();
std::vector<HintKey> gKeys = [] {
  std::vector<HintKey> keys;
  keys.reserve(gHints.size());
  for (const auto& hint : gHints) keys.push_back(make_key(hint));
  return keys;
}();

void rebuild_hints() {
  gHints = build_hints();
  gKeys.clear();
  gKeys.reserve(gHints.size());
  for (const auto& hint : gHints) gKeys.push_back(make_key(hint));
}

/// Scores only hints that actually match; skills match by name only and rank below commands,
/// so a large skill catalog cannot flood the menu or outrank a keyword-matched command.
int hint_score(const HintKey& key, const std::string& query) {
  if (query.empty()) return 1;
  int score = text_score(query, key.name);
  for (const auto& alias : key.aliases) score = std::max(score, text_score(query, alias));
  if (!key.skill && (key.text.find(query) != std::string::npos ||
                     subsequence_score(query, key.text) > 0)) {
    score = std::max(score, 20);
  }
  if (score == 0) return 0;
  if (!key.skill) return score;
  const int penalty = score >= 80 ? 10 : 40;
  return std::max(1, score - penalty);
}

bool conflicts_with_command(std::string_view name) {
  const std::string hint = "/" + std::string(name);
  const std::string padded = " " + std::string(name) + " ";
  return std::any_of(std::begin(kCommandHints), std::end(kCommandHints), [&](const CommandHint& command) {
    if (command.name == hint) return true;
    return (" " + command.aliases + " ").find(padded) != std::string::npos;
  });
}

}  // namespace

std::span<const CommandHint> command_hints() { return gHints; }

void load_file_commands(const std::filesystem::path& project, const std::filesystem::path& state) {
  gFileCommands.clear();
  const auto load = [](const std::filesystem::path& dir) {
    std::error_code error;
    for (const auto& entry : std::filesystem::directory_iterator(dir, error)) {
      if (!entry.is_regular_file() || entry.path().extension() != ".md") continue;
      auto command = read_file_command(entry.path());
      if (!command) continue;
      std::string name = lower_ascii(command->name);
      if (name.empty()) continue;
      if (std::any_of(std::begin(kCommandHints), std::end(kCommandHints),
                      [&](const CommandHint& hint) { return hint.name == "/" + name; })) continue;
      if (!gFileCommands.contains(name)) gFileCommands.emplace(std::move(name), std::move(*command));
    }
  };
  load(project / ".booth" / "commands");
  load(state / "commands");
  rebuild_hints();
}

std::vector<SkillInfo> load_skill_commands(const std::filesystem::path& cwd) {
  gSkills.clear();
  for (auto& skill : scan_default_skills(cwd)) {
    std::string name = lower_ascii(skill.name);
    if (name.empty() || conflicts_with_command(name) || gFileCommands.contains(name)) continue;
    gSkills.try_emplace(std::move(name), std::move(skill));
  }
  rebuild_hints();
  std::vector<SkillInfo> registered;
  registered.reserve(gSkills.size());
  for (const auto& [name, skill] : gSkills) registered.push_back(skill);
  return registered;
}

bool is_skill_prompt(const std::string& line) {
  const auto first = line.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || line[first] != '/') return false;
  const auto end = line.find_first_of(" \t\r\n", first);
  const std::string name = lower_ascii(line.substr(first + 1, end == std::string::npos ? std::string::npos : end - first - 1));
  return gSkills.contains(name);
}

std::optional<std::string> file_command_prompt(const std::string& line) {
  const auto first = line.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || line[first] != '/') return std::nullopt;
  const auto end = line.find_first_of(" \t\r\n", first);
  const std::string name = lower_ascii(line.substr(first + 1, end == std::string::npos ? std::string::npos : end - first - 1));
  const auto found = gFileCommands.find(name);
  if (found == gFileCommands.end()) return std::nullopt;
  const auto argument_start = end == std::string::npos ? std::string::npos : line.find_first_not_of(" \t", end);
  const std::string arguments = argument_start == std::string::npos ? std::string{} : line.substr(argument_start);
  return expand_file_command(found->second, arguments);
}

std::optional<std::string> slash_query(const std::string& composer) {
  const auto first = composer.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || composer[first] != '/') return std::nullopt;
  const std::string rest = composer.substr(first + 1);
  if (rest.find_first_of(" \t\r\n") != std::string::npos) return std::nullopt;
  return lower_ascii(rest);
}

bool command_name_matches(const CommandHint& hint, std::string_view query) {
  if (query.empty()) return true;
  if (text_score(query, lower_ascii(std::string(hint.name).substr(1))) > 0) return true;
  bool matched = false;
  for_each_alias(hint.aliases, [&](std::string_view alias) {
    if (!matched && text_score(query, lower_ascii(alias)) > 0) matched = true;
  });
  return matched;
}

std::string normalize_command(std::string line) {
  const auto first = line.find_first_not_of(" \t\r\n");
  if (first == std::string::npos || line[first] != '/') return line;
  // OMP's `/switch [model]` is a session-only model change: reuse the `/model`
  // picker and argument path so the two commands cannot drift apart.
  if (line.compare(first, 7, "/switch") == 0 &&
      (line.size() == first + 7 || std::isspace(static_cast<unsigned char>(line[first + 7])) != 0)) {
    return line.substr(0, first) + "/model" + line.substr(first + 7);
  }
  const auto end = line.find_first_of(" \t\r\n", first);
  const std::string token = line.substr(first, end == std::string::npos ? std::string::npos : end - first);
  const std::string name = token.substr(1);
  for (const auto& hint : command_hints()) {
    if (std::string_view(hint.name).substr(1) == name) return line;
    bool alias = false;
    for_each_alias(hint.aliases, [&](std::string_view candidate) {
      if (candidate == name) alias = true;
    });
    if (!alias) continue;
    return line.substr(0, first) + hint.name +
           (end == std::string::npos ? "" : line.substr(end));
  }
  return line;
}

std::vector<const CommandHint*> matching_commands(const std::string& composer) {
  const auto query = slash_query(composer);
  if (!query) return {};
  struct Ranked {
    int score;
    std::size_t name_length;
    bool skill;
    std::size_t order;
    const CommandHint* hint;
  };
  std::vector<Ranked> ranked;
  ranked.reserve(gHints.size());
  for (std::size_t index = 0; index < gHints.size(); ++index) {
    const int score = hint_score(gKeys[index], *query);
    if (score > 0) {
      ranked.push_back({score, std::string_view(gHints[index].name).size(), gHints[index].skill, index,
                        &gHints[index]});
    }
  }
  const bool prefer_short_names = !query->empty();
  std::stable_sort(ranked.begin(), ranked.end(), [prefer_short_names](const Ranked& left, const Ranked& right) {
    if (left.score != right.score) return left.score > right.score;
    if (prefer_short_names && left.name_length != right.name_length) {
      return left.name_length < right.name_length;
    }
    if (left.skill != right.skill) return !left.skill;
    return left.order < right.order;
  });
  std::vector<const CommandHint*> matches;
  matches.reserve(ranked.size());
  for (const auto& entry : ranked) matches.push_back(entry.hint);
  return matches;
}

}  // namespace booth
