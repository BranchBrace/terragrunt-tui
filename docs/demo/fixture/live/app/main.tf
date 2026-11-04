resource "terraform_data" "service" {
  input = "app-service"
}

resource "terraform_data" "task_definition" {
  input = "app-task_definition"
}

resource "terraform_data" "load_balancer" {
  input = "app-load_balancer"
}

resource "terraform_data" "listener" {
  input = "app-listener"
}

output "id" {
  value = "app"
}
