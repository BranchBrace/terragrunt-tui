resource "terraform_data" "vpc" {
  input = "network-vpc"
}

resource "terraform_data" "subnet_a" {
  input = "network-subnet_a"
}

resource "terraform_data" "subnet_b" {
  input = "network-subnet_b"
}

resource "terraform_data" "nat_gateway" {
  input = "network-nat_gateway"
}

output "id" {
  value = "network"
}
