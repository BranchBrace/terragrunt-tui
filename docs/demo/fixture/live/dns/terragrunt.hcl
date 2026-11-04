include "root" {
  path = find_in_parent_folders("root.hcl")
}

dependency "app" {
  config_path = "../app"
  mock_outputs = { id = "mock" }
}
