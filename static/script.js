let ws;

function showRegistration() {
    document.getElementById("login-container").classList.add("hidden");
    document.getElementById("registration-container").classList.remove("hidden");
}

function showLogin() {
    document.getElementById("registration-container").classList.add("hidden");
    document.getElementById("login-container").classList.remove("hidden");
}

async function register() {
    const username = document.getElementById("reg-username").value;
    const password = document.getElementById("reg-password").value;
    const messageDiv = document.getElementById("registration-message");

    if (!username || !password) {
        messageDiv.innerHTML = "<p class='error'>Username and password are required!</p>";
        return;
    }

    const response = await fetch("http://localhost:8080/register", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, password })
    });

    if (response.status === 201) {
        messageDiv.innerHTML = "<p class='success'>Registration successful! Redirecting to login...</p>";
        setTimeout(() => {
            document.getElementById("reg-username").value = "";
            document.getElementById("reg-password").value = "";
            showLogin();
        }, 1500);
    } else {
        messageDiv.innerHTML = "<p class='error'>Registration failed. Username might be taken.</p>";
    }
}

async function connect() {
    const username = document.getElementById("username").value;
    const password = document.getElementById("password").value;
    const mess = document.getElementById("error");

    if (!username || !password) {
        mess.innerText = "Username and password are required!";
        return;
    }

    const response = await fetch("http://localhost:8080/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ username, password })
    });

    if (response.status === 200) {
    } else if (response.status === 401) {
        mess.innerText = "Incorrect login or password";
        return;
    } else {
        mess.innerText = "Login error";
        return;
    }
    ws = new WebSocket(`ws://127.0.0.1:8080/ws?username=${username}&password=${password}`);

    ws.onopen = () => {
        document.getElementById("login-container").classList.add("hidden");
        document.getElementById("chat-container").classList.remove("hidden");
        document.getElementById("error").innerText = "";
    };

    ws.onmessage = (event) => {
        const message = event.data;
        console.log("Received message:", message); 

        if (message.startsWith("ACTIVE_USERS:")) {
            console.log("Handling active users update...");
            const users = JSON.parse(message.replace("ACTIVE_USERS: ", ""));
            updateList(users);
            return;
        }

        const chat = document.getElementById("chat");
        const messageDiv = document.createElement("div");
        messageDiv.textContent = message;
        chat.appendChild(messageDiv);

        chat.scrollTop = chat.scrollHeight;
    };
}

function sendMessage() {
    const input = document.getElementById("message");
    const message = input.value;

    if (!ws || ws.readyState !== WebSocket.OPEN) {
        console.error("WebSocket connection is not established or is not open!");
        return;
    }

    ws.send(message);
    input.value = '';
    console.log("Message sent:", message);
}

function updateList(users) {
    console.log("Updating list with users:", users); 
    const currentUser = document.getElementById("username").value;
    users.forEach(user => {
        if (user !== currentUser) { 
            const option = document.createElement("option");
            option.value = user;
            option.textContent = user;
        }
    });
}