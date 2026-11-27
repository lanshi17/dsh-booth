#include "interaction/btw_history.hpp"

#include <fstream>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using json = nlohmann::json;

}  // namespace

std::string btw_history_text(const std::filesystem::path& path, std::string_view session) {
  std::ifstream input(path);
  if (!input) return "no BTW history for this state directory";
  std::string line;
  std::vector<std::string> rows;
  while (std::getline(input, line)) {
    const auto document = json::parse(line, nullptr, false);
    if (document.is_discarded() || !document.is_object()) continue;
    const std::string owner = document.value("session", std::string{});
    if (!owner.empty() && owner != session) continue;
    const std::string question = document.value("question", std::string{});
    const std::string answer = document.value("answer", std::string{});
    if (question.empty() && answer.empty()) continue;
    rows.push_back("Q: " + question + "\nA: " + answer);
  }
  if (rows.empty()) return "no BTW history for this state directory";
  constexpr std::size_t kRows = 10;
  const std::size_t start = rows.size() > kRows ? rows.size() - kRows : 0;
  std::string out = rows.size() > kRows
                        ? "BTW history (last " + std::to_string(kRows) + " of " +
                              std::to_string(rows.size()) + ")"
                        : "BTW history";
  for (std::size_t index = start; index < rows.size(); ++index) out += "\n" + rows[index];
  return out;
}

void append_btw_history(const std::filesystem::path& path, std::string_view session,
                        const std::string& question, const std::string& answer) {
  std::error_code error;
  std::filesystem::create_directories(path.parent_path(), error);
  std::ofstream output(path, std::ios::app);
  if (!output) return;
  output << json{{"session", session}, {"question", question}, {"answer", answer}}.dump() << '\n';
  output.close();
  std::filesystem::permissions(path,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
}

}  // namespace booth
