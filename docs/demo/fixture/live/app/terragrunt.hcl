include "root" {
  path = find_in_parent_folders("root.hcl")
}

dependency "database" {
  config_path = "../database"
  mock_outputs = { id = "mock" }
}

dependency "cache" {
  config_path = "../cache"
  mock_outputs = { id = "mock" }
}
