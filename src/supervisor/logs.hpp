#pragma once

#include <cstddef>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

#include "contract/error.hpp"

namespace booth {

/// Append-only log with one rotated generation and a bounded tail read.
class Log {
 public:
  explicit Log(std::filesystem::path path, std::size_t max_bytes = 8u * 1024u * 1024u);

  Result<void> append(std::string_view line, std::string_view tag = {});
  Result<std::vector<std::string>> tail(std::size_t lines) const;

  [[nodiscard]] const std::filesystem::path& path() const { return path_; }

 private:
  void rotate_if_needed();

  std::filesystem::path path_;
  std::size_t max_bytes_;
};

}  // namespace booth
