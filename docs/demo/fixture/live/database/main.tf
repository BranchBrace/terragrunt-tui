resource "terraform_data" "cluster" {
  input = "database-cluster"
}

resource "terraform_data" "parameter_group" {
  input = "database-parameter_group"
}

output "id" {
  value = "database"
}
