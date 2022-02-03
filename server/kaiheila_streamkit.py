import json
import urllib.parse
import random
import asyncio
import websockets

import mimetypes

mimetypes.add_type('application/javascript', '.js')
mimetypes.add_type('text/css', '.css')

from flask import Flask, send_from_directory
import threading

with open('config.json', 'r', encoding='utf8') as f:
    config = json.load(f)
    guild_id = config['guild_id']
    channel_id = config['channel_id']
    client_id = config['client_id']
    auth_token = config['auth_token']

current_users = []
talking_users = []


def rand_id():
    return random.randint(1000000, 9999999)


async def hello(websocket):
    while True:
        await asyncio.sleep(0.1)
        await websocket.send(json.dumps({'current_users': current_users, 'talking_users': talking_users}))


async def start_client():
    global current_users
    global talking_users
    async with websockets.connect('ws://127.0.0.1:5988/?url=' + urllib.parse.quote_plus(
            'https://streamkit.kaiheila.cn/overlay/voice/%s/%s' % (guild_id, channel_id)),
                                  origin='https://streamkit.kaiheila.cn',
                                  subprotocols=['ws_streamkit']) as ws:
        print(await ws.recv())
        await ws.send(json.dumps(
            {"id": rand_id(), "args": {"client_id": client_id, "token": auth_token}, "cmd": "authenticate"}))
        await ws.recv()
        await ws.send(json.dumps(
            {"id": rand_id(), "args": {"channel_id": channel_id, "guild_id": guild_id}, "cmd": "subscribe",
             "evt": "audio_channel_user_change"}))
        await ws.send(json.dumps({"id": rand_id(), "args": {"channel_id": channel_id}, "cmd": "subscribe",
                                  "evt": "audio_channel_user_talk"}))
        while True:
            message = await ws.recv()
            data = json.loads(message)
            # print(data)

            if 'evt' in data:
                if data['evt'] == 'audio_channel_user_change':
                    current_users = data['data']
                    # filtered_users = []
                    # for user in current_users:
                    #     if user['id'] != '619621165':
                    #         filtered_users.append(user)
                    # current_users = filtered_users

                    print("Current users:")
                    for user in current_users:
                        print('- ' + user['nickname'])
                elif data['evt'] == 'audio_channel_user_talk':
                    talking_users = data['data']

                    for user in current_users:
                        user['talking'] = user['id'] in talking_users


# Flask server
app = Flask(__name__, static_url_path='/')


@app.route('/')
def serve_root():
    return send_from_directory('static', 'index.html')


if __name__ == "__main__":
    threading.Thread(target=lambda: app.run(host='0.0.0.0', port=5000, debug=True, use_reloader=False)).start()

    print("""
================================================

    启动成功 (*^▽^*)
    用浏览器或者 OBS 浏览器源打开 http://localhost:5000 即可。

================================================     
""")

    loop = asyncio.get_event_loop()
    server = websockets.serve(hello, "localhost", 8899)
    loop.run_until_complete(server)
    loop.run_until_complete(start_client())
    loop.run_forever()
