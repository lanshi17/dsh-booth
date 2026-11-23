#include "interaction/draft.hpp"

#include <fstream>
#include <system_error>

namespace booth {

std::string load_draft(const std::filesystem::path& path) {
  std::ifstream input(path, std::ios::binary);
  if (!input) return {};
  std::string text((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  return text;
}

void save_draft(const std::filesystem::path& path, const std::string& text) {
  std::error_code error;
  if (text.empty()) {
    std::filesystem::remove(path, error);
    return;
  }
  auto temporary = path;
  temporary += ".tmp";
  {
    std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
    if (!output) return;
    output.write(text.data(), static_cast<std::streamsize>(text.size()));
    output.close();
    if (!output) {
      std::filesystem::remove(temporary, error);
      return;
    }
  }
  std::filesystem::permissions(temporary,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
  std::filesystem::rename(temporary, path, error);
  if (error) {
    std::filesystem::remove(temporary, error);
  }
}

}  // namespace booth
