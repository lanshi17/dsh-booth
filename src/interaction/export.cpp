#include "interaction/export.hpp"

#include <chrono>
#include <ctime>
#include <fstream>
#include <string>

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

}  // namespace

std::string transcript_markdown(const AppState& state) {
  std::string out = "# booth transcript\n\n";
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
      case EntryKind::Notice:
        out += "> **notice**: " + entry.text + "\n\n";
        break;
      case EntryKind::Tool: {
        out += "### Tool: " + (entry.tool.empty() ? entry.text : entry.tool) + "\n\n";
        if (!entry.status.empty()) out += "- status: " + entry.status + "\n";
        const std::string took = duration(entry);
        if (!took.empty()) out += "- duration: " + took + "\n";
        if (!entry.input.empty()) out += "\nInput:\n\n```\n" + entry.input + "\n```\n";
        if (!entry.output.empty()) out += "\nOutput:\n\n```\n" + entry.output + "\n```\n";
        out += "\n";
        break;
      }
    }
  }
  return out;
}

void export_command(AppState& state, const std::string& line,
                    const std::filesystem::path& state_dir, const std::string& cwd) {
  std::filesystem::path target;
  if (line.size() > 7) {
    std::string path = line.substr(8);
    while (!path.empty() && (path.front() == ' ' || path.front() == '\t')) path.erase(path.begin());
    if (!path.empty()) {
      target = path;
      if (target.is_relative()) target = std::filesystem::path(cwd) / target;
    }
  }
  if (target.empty()) target = state_dir / ("transcript-" + stamp() + ".md");

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
  output << transcript_markdown(state);
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
