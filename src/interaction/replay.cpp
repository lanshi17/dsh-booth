#include "interaction/replay.hpp"

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <mutex>
#include <sstream>
#include <string>
#include <unordered_map>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using Json = nlohmann::json;

std::string text_of_content(const Json& content) {
  if (content.is_string()) return content.get<std::string>();
  if (content.is_object()) {
    if (content.contains("content")) return text_of_content(content["content"]);
    if (content.contains("text") && content["text"].is_string()) {
      return content["text"].get<std::string>();
    }
    return {};
  }
  if (!content.is_array()) return {};
  std::string text;
  const auto append = [&](const std::string& part) {
    if (part.empty()) return;
    if (!text.empty() && text.back() != '\n') text += '\n';
    text += part;
  };
  for (const auto& block : content) {
    if (!block.is_object()) continue;
    const std::string type = block.value("type", std::string{});
    if (type == "text") {
      append(block.value("text", std::string{}));
    } else if (type == "content") {
      append(text_of_content(block.value("content", Json{})));
    } else if (block.contains("text") && block["text"].is_string()) {
      append(block["text"].get<std::string>());
    }
  }
  return text;
}

std::string tool_input_of(const Json& arguments) {
  if (arguments.is_string()) {
    const auto parsed = Json::parse(arguments.get<std::string>(), nullptr, false);
    if (!parsed.is_discarded() && parsed.is_object()) return tool_input_of(parsed);
    return arguments.get<std::string>();
  }
  if (arguments.is_object()) {
    for (const char* key : {"command", "cmd", "path", "file_path", "pattern", "query", "url"}) {
      if (arguments.contains(key) && arguments[key].is_string()) {
        return arguments[key].get<std::string>();
      }
    }
    return arguments.dump();
  }
  return {};
}

std::chrono::system_clock::time_point at_millis(long long millis) {
  return std::chrono::system_clock::time_point{std::chrono::milliseconds{millis}};
}

std::filesystem::path find_session_file(const std::filesystem::path& sessions_dir,
                                        const std::string& session_id) {
  std::error_code error;
  if (!std::filesystem::is_directory(sessions_dir, error)) return {};
  std::filesystem::path file;
  const auto find_file = [&](const std::filesystem::path& dir) {
    std::error_code local;
    if (!std::filesystem::is_directory(dir, local)) return false;
    for (const auto& candidate : std::filesystem::directory_iterator(dir, local)) {
      if (!candidate.is_regular_file(local)) continue;
      if (candidate.path().filename().string().rfind("session", 0) == 0 &&
          candidate.path().extension() == ".zstd") {
        file = candidate.path();
        return true;
      }
    }
    return false;
  };
  if (!find_file(sessions_dir / session_id)) {
    for (const auto& project : std::filesystem::directory_iterator(
             sessions_dir, std::filesystem::directory_options::skip_permission_denied, error)) {
      if (error) {
        error.clear();
        continue;
      }
      if (!project.is_directory(error)) continue;
      if (find_file(project.path() / session_id)) break;
    }
  }
  return file;
}

struct CacheEntry {
  std::filesystem::file_time_type mtime;
  std::uintmax_t size{0};
  std::vector<TranscriptEntry> entries;
};

std::mutex cache_mutex;
std::unordered_map<std::string, CacheEntry> replay_cache;

struct TitleEntry {
  std::filesystem::file_time_type mtime;
  std::uintmax_t size{0};
  std::string title;
};

std::unordered_map<std::string, TitleEntry> title_cache;

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

}  // namespace

std::vector<TranscriptEntry> parse_session_jsonl(std::istream& input, std::size_t limit) {
  std::vector<TranscriptEntry> entries;
  std::unordered_map<std::string, std::size_t> tool_rows;
  const auto find_tool = [&](const std::string& id) -> TranscriptEntry* {
    const auto found = tool_rows.find(id);
    return found == tool_rows.end() ? nullptr : &entries[found->second];
  };
  std::string line;
  while (std::getline(input, line)) {
    const auto event = Json::parse(line, nullptr, false);
    if (event.is_discarded() || !event.is_object()) continue;
    const std::string type = event.value("type", std::string{});
    const auto data = event.value("data", Json::object());

    if (type == "user/message") {
      const auto source = data.value("source", Json::object());
      const std::string kind = source.is_object() ? source.value("kind", std::string{}) : "";
      if (kind != "user" && source.is_object()) continue;
      const std::string text = text_of_content(data.value("content", Json::array()));
      if (text.empty()) continue;
      TranscriptEntry entry;
      entry.kind = EntryKind::User;
      entry.text = text;
      entries.push_back(std::move(entry));
      continue;
    }

    if (type == "tool/call") {
      TranscriptEntry entry;
      entry.kind = EntryKind::Tool;
      entry.id = data.value("callId", std::string{});
      entry.tool = data.value("name", std::string{"tool"});
      entry.text = entry.tool;
      entry.input = tool_input_of(data.value("arguments", Json{}));
      entry.status = "running";
      const long long time = event.value("time", 0LL);
      if (time > 0) entry.started = at_millis(time);
      tool_rows[entry.id] = entries.size();
      entries.push_back(std::move(entry));
      continue;
    }

    if (type == "tool/result") {
      const auto message = data.value("message", Json::object());
      const std::string id = message.value("toolCallId", std::string{});
      TranscriptEntry* entry = find_tool(id);
      if (entry == nullptr) {
        TranscriptEntry created;
        created.kind = EntryKind::Tool;
        created.id = id;
        created.tool = "tool";
        created.text = "tool";
        entries.push_back(std::move(created));
        tool_rows[id] = entries.size() - 1;
        entry = &entries.back();
      }
      entry->output = text_of_content(message.value("content", Json::array()));
      entry->status = message.value("isError", false) ? "failed" : "completed";
      const long long time = event.value("time", 0LL);
      if (time > 0) entry->finished = at_millis(time);
      continue;
    }

    if (type == "assistant/message") {
      const auto message = data.value("message", Json::object());
      const auto content = message.value("content", Json::array());
      for (const auto& block : content) {
        if (!block.is_object()) continue;
        const std::string block_type = block.value("type", std::string{});
        const std::string text = block.value("text", std::string{});
        if (text.empty()) continue;
        if (block_type == "text") {
          TranscriptEntry entry;
          entry.kind = EntryKind::Assistant;
          entry.text = text;
          entries.push_back(std::move(entry));
        } else if (block_type == "reasoning") {
          TranscriptEntry entry;
          entry.kind = EntryKind::Thought;
          entry.text = text;
          entries.push_back(std::move(entry));
        }
      }
    }
  }
  if (entries.size() > limit) entries.erase(entries.begin(), entries.end() - limit);
  return entries;
}

std::string session_title(const std::filesystem::path& sessions_dir,
                          const std::string& session_id) {
  const std::filesystem::path file = find_session_file(sessions_dir, session_id);
  if (file.empty()) return {};
  std::error_code stat_error;
  const auto mtime = std::filesystem::last_write_time(file, stat_error);
  const auto size = std::filesystem::file_size(file, stat_error);
  const std::string key = file.string();
  {
    std::lock_guard<std::mutex> lock(cache_mutex);
    const auto found = title_cache.find(key);
    if (found != title_cache.end() && found->second.mtime == mtime &&
        found->second.size == size) {
      return found->second.title;
    }
  }
  const std::string command =
      "zstd -dc -- " + shell_quote(file.string()) + " 2>/dev/null | head -n 24";
  FILE* pipe = ::popen(command.c_str(), "r");
  if (pipe == nullptr) return {};
  std::string output;
  char buffer[4096];
  while (std::fgets(buffer, sizeof(buffer), pipe) != nullptr) output += buffer;
  ::pclose(pipe);
  std::istringstream input(output);
  std::string line;
  while (std::getline(input, line)) {
    const auto event = Json::parse(line, nullptr, false);
    if (event.is_discarded() || event.value("type", std::string{}) != "session/title") continue;
    const auto data = event.value("data", Json::object());
    const std::string title = data.value("title", std::string{});
    if (!title.empty()) {
      std::lock_guard<std::mutex> lock(cache_mutex);
      if (title_cache.size() >= 64) title_cache.erase(title_cache.begin());
      title_cache[key] = TitleEntry{mtime, size, title};
      return title;
    }
  }
  return {};
}

Result<std::vector<TranscriptEntry>> replay_session(const std::filesystem::path& sessions_dir,
                                                    const std::string& session_id) {
  std::error_code error;
  const std::filesystem::path file = find_session_file(sessions_dir, session_id);
  if (file.empty()) return {};
  const auto mtime = std::filesystem::last_write_time(file, error);
  const auto size = std::filesystem::file_size(file, error);
  error.clear();
  const std::string key = sessions_dir.string() + "|" + session_id;
  {
    std::lock_guard<std::mutex> lock(cache_mutex);
    const auto found = replay_cache.find(key);
    if (found != replay_cache.end() && found->second.mtime == mtime &&
        found->second.size == size) {
      return found->second.entries;
    }
  }

  const std::string command =
      "zstd -dc -- " + shell_quote(file.string()) + " 2>/dev/null";
  FILE* pipe = ::popen(command.c_str(), "r");
  if (pipe == nullptr) {
    return std::unexpected(Error{Layer::Process, "cannot run zstd", std::nullopt});
  }
  std::string output;
  char buffer[8192];
  while (std::fgets(buffer, sizeof(buffer), pipe) != nullptr) output += buffer;
  const int status = ::pclose(pipe);
  if (status != 0) {
    return std::unexpected(Error{Layer::Process, "cannot decompress session", status});
  }
  if (output.empty()) return std::vector<TranscriptEntry>{};

  std::istringstream input(output);
  auto entries = parse_session_jsonl(input);
  {
    std::lock_guard<std::mutex> lock(cache_mutex);
    if (replay_cache.size() >= 8) replay_cache.erase(replay_cache.begin());
    replay_cache[key] = CacheEntry{mtime, size, entries};
  }
  return entries;
}

}  // namespace booth
